//! AES-256-GCM token encryption.
//!
//! Encrypts AuthSession data at rest using a random 32-byte key file.
//! The key file is created with restricted permissions (0o600 on Unix).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rmccc_error::{RmcccError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::types::AuthSession;

/// Encrypted payload stored on disk.
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedPayload {
    nonce: String,     // base64-encoded 12-byte nonce
    ciphertext: String, // base64-encoded ciphertext + auth tag
}

/// Manages encryption and decryption of auth tokens.
pub struct TokenVault {
    cipher: Aes256Gcm,
    #[allow(dead_code)]
    key_path: PathBuf,
}

impl TokenVault {
    /// Load existing key or generate a new one.
    pub fn init(key_path: &Path) -> Result<Self> {
        let key_bytes = if key_path.exists() {
            std::fs::read(key_path)?
        } else {
            // Generate random 32-byte key
            let mut key = [0u8; 32];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut key);
            std::fs::write(key_path, &key)?;

            // Restrict permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))?;
            }

            key.to_vec()
        };

        if key_bytes.len() != 32 {
            return Err(RmcccError::Other(format!(
                "Invalid key file size: {} bytes (expected 32)",
                key_bytes.len()
            )));
        }

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| RmcccError::Other(format!("Failed to init cipher: {e}")))?;

        Ok(Self {
            cipher,
            key_path: key_path.to_path_buf(),
        })
    }

    /// Encrypt an AuthSession to a JSON string suitable for writing to auth.json.
    pub fn encrypt(&self, session: &AuthSession) -> Result<String> {
        let mut nonce_bytes = [0u8; 12];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let plaintext = serde_json::to_vec(session)?;
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
            .map_err(|e| RmcccError::Other(format!("Encryption failed: {e}")))?;

        let payload = EncryptedPayload {
            nonce: B64.encode(nonce_bytes),
            ciphertext: B64.encode(ciphertext),
        };

        Ok(serde_json::to_string_pretty(&payload)?)
    }

    /// Decrypt a JSON string from auth.json back to an AuthSession.
    pub fn decrypt(&self, data: &str) -> Result<AuthSession> {
        let payload: EncryptedPayload = serde_json::from_str(data)?;

        let nonce_bytes = B64
            .decode(&payload.nonce)
            .map_err(|e| RmcccError::Other(format!("Invalid nonce encoding: {e}")))?;
        let ciphertext = B64
            .decode(&payload.ciphertext)
            .map_err(|e| RmcccError::Other(format!("Invalid ciphertext encoding: {e}")))?;

        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| RmcccError::Other("Decryption failed: wrong key or corrupted data".into()))?;

        Ok(serde_json::from_slice(&plaintext)?)
    }

    /// Save an AuthSession to the auth.json file (encrypted).
    pub fn save_session(&self, session: &AuthSession, auth_path: &Path) -> Result<()> {
        let encrypted = self.encrypt(session)?;
        std::fs::write(auth_path, encrypted)?;
        tracing::info!("Auth session saved to {}", auth_path.display());
        Ok(())
    }

    /// Load an AuthSession from the auth.json file (decrypted).
    pub fn load_session(&self, auth_path: &Path) -> Result<Option<AuthSession>> {
        if !auth_path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(auth_path)?;
        match self.decrypt(&data) {
            Ok(session) => {
                tracing::info!("Auth session loaded for {}", session.username);
                Ok(Some(session))
            }
            Err(e) => {
                tracing::warn!("Failed to decrypt auth.json: {e}");
                Ok(None)
            }
        }
    }

    /// Delete the auth.json file (logout).
    pub fn clear_session(&self, auth_path: &Path) -> Result<()> {
        if auth_path.exists() {
            std::fs::remove_file(auth_path)?;
            tracing::info!("Auth session cleared");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let tmp = std::env::temp_dir().join("rmccc_test_key");
        let _ = std::fs::remove_file(&tmp); // clean up

        let vault = TokenVault::init(&tmp).unwrap();
        let session = AuthSession {
            provider: "test".into(),
            access_token: "secret_token_12345".into(),
            username: "TestUser".into(),
            uuid: "test-uuid".into(),
            refresh_token: Some("refresh_token_67890".into()),
            expires_at: None,
        };

        let encrypted = vault.encrypt(&session).unwrap();
        let decrypted = vault.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted.username, "TestUser");
        assert_eq!(decrypted.access_token, "secret_token_12345");
        assert_eq!(decrypted.refresh_token, Some("refresh_token_67890".into()));

        let _ = std::fs::remove_file(&tmp);
    }
}
