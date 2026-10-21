//! AES-256-GCM token encryption.
//!
//! Encrypts JSON payloads at rest using a random 32-byte key file. The key file
//! is created with owner-only permissions via [`PlatformFs::restrict_owner_only`]
//! (Unix `0600` / Windows ACL). This is the single encryption primitive;
//! storage-level concerns belong to [`AccountRepository`](crate::AccountRepository).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::types::AuthSession;

/// Envelope format version marker for forward-compatible migration of the on-disk layout.
const ENVELOPE_VERSION: u32 = 1;

/// Encrypted payload stored on disk.
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedPayload {
    /// Envelope format version. Old files without this member deserialize as
    /// version 1, which matches the original `{ nonce, ciphertext }` layout.
    #[serde(default = "default_envelope_version")]
    version: u32,
    nonce: String,      // base64-encoded 12-byte nonce
    ciphertext: String, // base64-encoded ciphertext + auth tag
}

fn default_envelope_version() -> u32 {
    ENVELOPE_VERSION
}

/// Manages encryption and decryption of JSON payloads.
pub struct TokenVault {
    cipher: Aes256Gcm,
    #[allow(dead_code)]
    key_path: PathBuf,
}

impl TokenVault {
    /// Load existing key or generate a new one.
    ///
    /// 权限收紧经注入的 [`PlatformFs`]（Unix `0600` / Windows ACL），本函数
    /// 不再出现 `#[cfg(unix)]` / `#[cfg(windows)]`（指南 §5.7.1）。
    pub fn init(key_path: &Path, fs: &dyn PlatformFs) -> Result<Self> {
        let key_bytes = if key_path.exists() {
            std::fs::read(key_path)?
        } else {
            // Generate random 32-byte key
            let mut key = [0u8; 32];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut key);
            std::fs::write(key_path, key)?;

            // 仅属主可读写。密钥文件权限收紧失败必须报错——悄悄放过等于把
            // 令牌明文暴露给同机其他用户。
            fs.restrict_owner_only(key_path)?;

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

    /// Encrypt an arbitrary serializable payload into a versioned envelope string.
    ///
    /// This is the single encryption primitive; storage-level concerns belong to
    /// [`AccountRepository`](crate::AccountRepository).
    pub fn encrypt_payload<T: Serialize>(&self, value: &T) -> Result<String> {
        let mut nonce_bytes = [0u8; 12];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let plaintext = serde_json::to_vec(value)?;
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
            .map_err(|e| RmcccError::Other(format!("Encryption failed: {e}")))?;

        let payload = EncryptedPayload {
            version: ENVELOPE_VERSION,
            nonce: B64.encode(nonce_bytes),
            ciphertext: B64.encode(ciphertext),
        };

        Ok(serde_json::to_string_pretty(&payload)?)
    }

    /// Decrypt a versioned envelope string back into a typed payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the envelope version is unsupported or the payload
    /// cannot be decrypted (wrong key or corrupted data).
    pub fn decrypt_payload<T: serde::de::DeserializeOwned>(&self, data: &str) -> Result<T> {
        let payload: EncryptedPayload = serde_json::from_str(data)?;
        if payload.version != ENVELOPE_VERSION {
            return Err(RmcccError::Other(format!(
                "Unsupported encrypted payload version: {} (expected {})",
                payload.version, ENVELOPE_VERSION
            )));
        }

        let nonce_bytes = B64
            .decode(&payload.nonce)
            .map_err(|e| RmcccError::Other(format!("Invalid nonce encoding: {e}")))?;
        let ciphertext = B64
            .decode(&payload.ciphertext)
            .map_err(|e| RmcccError::Other(format!("Invalid ciphertext encoding: {e}")))?;

        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
            .map_err(|_| {
                RmcccError::Other("Decryption failed: wrong key or corrupted data".into())
            })?;

        Ok(serde_json::from_slice(&plaintext)?)
    }

    /// Convenience wrapper encrypting an [`AuthSession`] (used by legacy
    /// migration in [`AccountRepository::migrate_legacy`]).
    pub fn encrypt(&self, session: &AuthSession) -> Result<String> {
        self.encrypt_payload(session)
    }

    /// Convenience wrapper decrypting an [`AuthSession`] (used by legacy
    /// migration in [`AccountRepository::migrate_legacy`]).
    pub fn decrypt(&self, data: &str) -> Result<AuthSession> {
        self.decrypt_payload(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_id::AccountID;
    use rmccc_platform::Platform;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let tmp = std::env::temp_dir().join("rmccc_test_key");
        let _ = std::fs::remove_file(&tmp); // clean up

        let vault = TokenVault::init(&tmp, Platform::current().fs.as_ref()).unwrap();
        let session = AuthSession {
            account_id: AccountID::generate().to_string(),
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

    #[test]
    fn test_encrypt_payload_is_versioned() {
        let tmp = std::env::temp_dir().join("rmccc_test_key_v");
        let _ = std::fs::remove_file(&tmp);

        let vault = TokenVault::init(&tmp, Platform::current().fs.as_ref()).unwrap();
        let encrypted = vault
            .encrypt_payload(&serde_json::json!({"k": "v"}))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        assert_eq!(parsed["version"], 1);
        assert!(parsed["nonce"].is_string());
        assert!(parsed["ciphertext"].is_string());

        // No plaintext leakage in the envelope.
        assert!(!encrypted.contains("v\""));

        let _ = std::fs::remove_file(&tmp);
    }
}
