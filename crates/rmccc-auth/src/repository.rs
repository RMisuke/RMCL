//! Account repository — plaintext metadata + encrypted private data storage.
//!
//! Mirrors HMCL's `Accounts`: `accounts.json` stores public metadata (no
//! credentials) so the account list can be rendered without decryption, while
//! `private/account-private-data.json` stores the encrypted credential payload
//! keyed by the stable [`AccountID`]. All read-modify-write operations are
//! serialized by an internal mutex so concurrent IPC commands cannot corrupt
//! the store.

use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use ts_rs::TS;

use crate::account::Account;
use crate::account_id::AccountID;
use crate::factory::AccountFactory;
use crate::types::AuthSession;
use crate::vault::TokenVault;

/// On-disk schema version of `accounts.json`.
const METADATA_VERSION: u32 = 1;

/// Public account metadata (no credentials), stored in plaintext `accounts.json`.
///
/// Crosses the IPC boundary so the frontend can render the account list without
/// ever receiving tokens.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AccountMetadata {
    /// The authentication provider id (`microsoft`, `offline`, ...).
    pub r#type: String,
    /// The stable launcher account record ID (`account:<uuid>`).
    pub account_id: String,
    /// The in-game profile name.
    pub profile_name: String,
    /// The Minecraft profile UUID.
    pub profile_id: String,
}

/// Top-level `accounts.json` layout.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataStore {
    /// Schema version for forward-compatible migration.
    #[serde(default = "default_metadata_version")]
    version: u32,
    /// The currently selected account ID (`account:<uuid>`), if any.
    #[serde(default)]
    selected: Option<String>,
    /// Public metadata for all accounts.
    #[serde(default)]
    accounts: Vec<AccountMetadata>,
}

fn default_metadata_version() -> u32 {
    METADATA_VERSION
}

/// Private credential data stored encrypted, keyed by account ID.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PrivateStore {
    accounts: BTreeMap<String, PrivateAccountData>,
}

/// Encrypted contents of a single account's credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrivateAccountData {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Repository for persisted accounts: plaintext metadata + encrypted private data.
pub struct AccountRepository {
    vault: Arc<TokenVault>,
    data_dir: PathBuf,
    /// Serializes all read-modify-write operations on the store files.
    write_lock: Mutex<()>,
    /// Login-type factories (aligned with HMCL `Accounts`), keyed by
    /// [`AccountFactory::login_type`]. `create` performs a fresh login;
    /// `from_storage` restores a persisted account so it can be lazily
    /// refreshed via [`Account::log_in`].
    factories: HashMap<String, Box<dyn AccountFactory>>,
}

impl AccountRepository {
    /// Creates a repository rooted at `data_dir`, ensuring the `private/`
    /// directory exists.
    pub fn new(data_dir: &Path, vault: Arc<TokenVault>) -> Result<Self> {
        std::fs::create_dir_all(data_dir.join("private"))?;
        Ok(Self {
            vault,
            data_dir: data_dir.to_path_buf(),
            write_lock: Mutex::new(()),
            factories: HashMap::new(),
        })
    }

    /// Registers a login-type factory (e.g. offline / microsoft). Must be
    /// called before accounts of that type are restored via
    /// [`AccountRepository::load_account`].
    pub fn register_factory(&mut self, factory: Box<dyn AccountFactory>) {
        tracing::info!(
            login_type = factory.login_type(),
            "Account factory registered"
        );
        self.factories
            .insert(factory.login_type().to_string(), factory);
    }

    /// Returns the factory registered for a login type, or an error when none
    /// is registered.
    pub fn factory(&self, login_type: &str) -> Result<&dyn AccountFactory> {
        self.factories
            .get(login_type)
            .map(|f| f.as_ref())
            .ok_or_else(|| {
                RmcccError::Other(format!(
                    "No account factory registered for login type '{login_type}'"
                ))
            })
    }

    /// Returns the underlying vault (shared with the caller).
    pub fn vault(&self) -> &Arc<TokenVault> {
        &self.vault
    }

    /// The plaintext metadata file path (`{data_dir}/accounts.json`).
    pub fn metadata_path(&self) -> PathBuf {
        self.data_dir.join("accounts.json")
    }

    /// The encrypted private data file path
    /// (`{data_dir}/private/account-private-data.json`).
    pub fn private_path(&self) -> PathBuf {
        self.data_dir
            .join("private")
            .join("account-private-data.json")
    }

    // ── low-level read/write ──

    fn read_metadata(&self) -> Result<MetadataStore> {
        let path = self.metadata_path();
        if !path.exists() {
            return Ok(MetadataStore::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn write_metadata(&self, store: &MetadataStore) -> Result<()> {
        atomic_write_json(&self.metadata_path(), store)
    }

    fn read_private(&self) -> Result<PrivateStore> {
        let path = self.private_path();
        if !path.exists() {
            return Ok(PrivateStore::default());
        }
        let content = std::fs::read_to_string(&path)?;
        self.vault.decrypt_payload(&content)
    }

    fn write_private(&self, store: &PrivateStore) -> Result<()> {
        let encrypted = self.vault.encrypt_payload(store)?;
        atomic_write_str(&self.private_path(), &encrypted)
    }

    // ── public API ──

    /// Returns public metadata for all accounts (plaintext, no decryption).
    pub fn list_metadata(&self) -> Result<Vec<AccountMetadata>> {
        Ok(self.read_metadata()?.accounts)
    }

    /// Returns the selected account ID, if any.
    pub fn selected_account_id(&self) -> Result<Option<String>> {
        Ok(self.read_metadata()?.selected)
    }

    /// Persists a session: upserts its metadata and private data, and selects
    /// it when it is the first account or is already the selected one.
    pub fn save(&self, session: &AuthSession) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("Account repository write lock poisoned".into()))?;

        let account_id = session.account_id.clone();
        if AccountID::parse(&account_id).is_err() {
            return Err(RmcccError::Other(format!(
                "Refusing to persist session with invalid account ID: {account_id}"
            )));
        }

        let mut meta = self.read_metadata()?;
        match meta
            .accounts
            .iter_mut()
            .find(|m| m.account_id == account_id)
        {
            Some(m) => {
                m.r#type = session.provider.clone();
                m.profile_name = session.username.clone();
                m.profile_id = session.uuid.clone();
            }
            None => {
                meta.accounts.push(AccountMetadata {
                    r#type: session.provider.clone(),
                    account_id: account_id.clone(),
                    profile_name: session.username.clone(),
                    profile_id: session.uuid.clone(),
                });
            }
        }
        if meta.selected.is_none() || meta.selected.as_deref() == Some(&account_id) {
            meta.selected = Some(account_id.clone());
        }
        self.write_metadata(&meta)?;

        let mut priv_store = self.read_private()?;
        priv_store.accounts.insert(
            account_id.clone(),
            PrivateAccountData {
                access_token: session.access_token.clone(),
                refresh_token: session.refresh_token.clone(),
                expires_at: session.expires_at,
            },
        );
        self.write_private(&priv_store)?;

        tracing::info!(
            account_id = %account_id,
            username = %session.username,
            provider = %session.provider,
            "Account saved"
        );
        Ok(())
    }

    /// Loads and decrypts a single account by its stable account ID.
    pub fn load(&self, account_id: &str) -> Result<Option<AuthSession>> {
        let meta = self.read_metadata()?;
        let m = match meta.accounts.iter().find(|m| m.account_id == account_id) {
            Some(m) => m,
            None => return Ok(None),
        };
        let priv_store = self.read_private()?;
        let p = match priv_store.accounts.get(account_id) {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(Some(AuthSession {
            account_id: m.account_id.clone(),
            provider: m.r#type.clone(),
            username: m.profile_name.clone(),
            uuid: m.profile_id.clone(),
            access_token: p.access_token.clone(),
            refresh_token: p.refresh_token.clone(),
            expires_at: p.expires_at,
        }))
    }

    /// Loads and decrypts the currently selected account, if any.
    pub fn load_active(&self) -> Result<Option<AuthSession>> {
        match self.selected_account_id()? {
            Some(id) => self.load(&id),
            None => Ok(None),
        }
    }

    /// Restores a persisted account into a live [`Account`] via its factory.
    ///
    /// Credentials may be expired; callers refresh lazily through
    /// [`Account::log_in`] (P2: token lifecycle lives inside the account).
    ///
    /// # Errors
    ///
    /// Returns an error when the account has no registered factory or the
    /// stored data is malformed.
    pub fn load_account(&self, account_id: &str) -> Result<Option<Box<dyn Account>>> {
        let session = match self.load(account_id)? {
            Some(s) => s,
            None => return Ok(None),
        };
        let factory = self.factories.get(&session.provider).ok_or_else(|| {
            RmcccError::Other(format!(
                "No account factory registered for provider '{}'",
                session.provider
            ))
        })?;
        let metadata = serde_json::json!({
            "type": session.provider.clone(),
            "account_id": session.account_id.clone(),
            "profile_name": session.username.clone(),
            "profile_id": session.uuid.clone(),
        });
        let private = serde_json::json!({
            "access_token": session.access_token.clone(),
            "refresh_token": session.refresh_token.clone(),
            "expires_at": session.expires_at,
        });
        factory.from_storage(&metadata, Some(&private)).map(Some)
    }

    /// Restores the currently selected account into a live [`Account`].
    pub fn load_active_account(&self) -> Result<Option<Box<dyn Account>>> {
        match self.selected_account_id()? {
            Some(id) => self.load_account(&id),
            None => Ok(None),
        }
    }

    /// Persists an [`Account`] back to storage (metadata + encrypted private
    /// data) after a refresh or re-authorization.
    pub fn save_account(&self, account: &dyn Account) -> Result<()> {
        self.save(&account.to_session())
    }

    /// Returns all accounts as decrypted sessions (IPC compatibility helper).
    pub fn list_sessions(&self) -> Result<Vec<AuthSession>> {
        let meta = self.read_metadata()?;
        let priv_store = self.read_private()?;
        let mut out = Vec::with_capacity(meta.accounts.len());
        for m in &meta.accounts {
            if let Some(p) = priv_store.accounts.get(&m.account_id) {
                out.push(AuthSession {
                    account_id: m.account_id.clone(),
                    provider: m.r#type.clone(),
                    username: m.profile_name.clone(),
                    uuid: m.profile_id.clone(),
                    access_token: p.access_token.clone(),
                    refresh_token: p.refresh_token.clone(),
                    expires_at: p.expires_at,
                });
            }
        }
        Ok(out)
    }

    /// Removes an account (metadata + private data). When the removed account
    /// was selected, the first remaining account becomes selected.
    pub fn remove(&self, account_id: &str) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("Account repository write lock poisoned".into()))?;

        let mut meta = self.read_metadata()?;
        let existed = meta.accounts.iter().any(|m| m.account_id == account_id);
        meta.accounts.retain(|m| m.account_id != account_id);
        if meta.selected.as_deref() == Some(account_id) {
            meta.selected = meta.accounts.first().map(|m| m.account_id.clone());
        }
        self.write_metadata(&meta)?;

        let mut priv_store = self.read_private()?;
        priv_store.accounts.remove(account_id);
        self.write_private(&priv_store)?;

        if existed {
            tracing::info!(account_id = %account_id, "Account removed");
        }
        Ok(())
    }

    /// Selects an account as the active one.
    ///
    /// # Errors
    ///
    /// Returns [`RmcccError::ProfileNotFound`] when the account ID does not exist.
    pub fn select(&self, account_id: &str) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("Account repository write lock poisoned".into()))?;

        let mut meta = self.read_metadata()?;
        if !meta.accounts.iter().any(|m| m.account_id == account_id) {
            return Err(RmcccError::ProfileNotFound(account_id.to_string()));
        }
        meta.selected = Some(account_id.to_string());
        self.write_metadata(&meta)?;
        tracing::info!(account_id = %account_id, "Selected account switched");
        Ok(())
    }

    /// Migrates legacy storage into the new layout.
    ///
    /// Supports the two legacy formats that predate the repository:
    /// - `{data_dir}/accounts/{uuid}.json` — the multi-account directory with
    ///   an `active` pointer (P2-10, now replaced by `accounts.json`).
    /// - `{data_dir}/auth.json` — the original single encrypted session file.
    ///
    /// Legacy files are archived with a `.legacy` suffix instead of being
    /// deleted so they remain available for diagnosis. Returns `true` when any
    /// legacy data was migrated. Idempotent: when no legacy files are present
    /// it does nothing.
    pub fn migrate_legacy(&self) -> Result<bool> {
        let accounts_dir = self.data_dir.join("accounts");
        let auth_json = self.data_dir.join("auth.json");
        let mut migrated = false;

        // 1) Legacy multi-account directory.
        if accounts_dir.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&accounts_dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
                .collect();
            entries.sort();
            for path in entries {
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Skipping unreadable legacy account file"
                        );
                        continue;
                    }
                };
                match self.vault.decrypt(&content) {
                    Ok(session) => {
                        self.save(&session)?;
                        migrated = true;
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Skipping undecryptable legacy account file"
                        );
                    }
                }
            }
            let archived = self.data_dir.join("accounts.legacy");
            let _ = std::fs::rename(&accounts_dir, &archived);
        }

        // 2) Legacy single-file auth.json.
        if auth_json.exists() {
            let content = std::fs::read_to_string(&auth_json)?;
            match self.vault.decrypt(&content) {
                Ok(session) => {
                    self.save(&session)?;
                    migrated = true;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to migrate legacy auth.json");
                }
            }
            let _ = std::fs::rename(&auth_json, self.data_dir.join("auth.json.legacy"));
        }

        if migrated {
            tracing::info!("Legacy account storage migrated to the new layout");
        }
        Ok(migrated)
    }
}

/// Atomically writes a serializable value as pretty JSON (tmp + rename).
fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    atomic_write_str(path, &content)
}

/// Atomically writes a string to `path` (tmp + rename) after creating parents.
fn atomic_write_str(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-repo-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn session(provider: &str, username: &str) -> AuthSession {
        AuthSession {
            account_id: AccountID::generate().to_string(),
            provider: provider.into(),
            access_token: format!("token-{username}"),
            username: username.into(),
            uuid: crate::offline::OfflineAuthProvider::uuid_from_username(username),
            refresh_token: Some(format!("refresh-{username}")),
            expires_at: Some(chrono::Utc::now() + Duration::from_secs(3600)),
        }
    }

    #[test]
    fn test_save_load_active_and_list() {
        let dir = temp_dir("basic");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let repo = AccountRepository::new(&dir, vault).unwrap();

        assert!(repo.load_active().unwrap().is_none());
        assert!(repo.list_metadata().unwrap().is_empty());

        let a = session("offline", "Alice");
        repo.save(&a).unwrap();
        // First account becomes selected.
        assert_eq!(
            repo.selected_account_id().unwrap().as_deref(),
            Some(a.account_id.as_str())
        );
        // Metadata is stored plaintext (readable without the key).
        let metadata = repo.list_metadata().unwrap();
        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].profile_name, "Alice");

        let loaded = repo.load(&a.account_id).unwrap().unwrap();
        assert_eq!(loaded.username, "Alice");
        assert_eq!(loaded.access_token, "token-Alice");
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-Alice"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_multi_account_and_switch() {
        let dir = temp_dir("multi");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let repo = AccountRepository::new(&dir, vault).unwrap();

        let a = session("offline", "Alice");
        let b = session("offline", "Bob");
        repo.save(&a).unwrap();
        repo.save(&b).unwrap();
        // Adding a second account must not steal the selection.
        assert_eq!(
            repo.selected_account_id().unwrap().as_deref(),
            Some(a.account_id.as_str())
        );

        repo.select(&b.account_id).unwrap();
        assert_eq!(
            repo.selected_account_id().unwrap().as_deref(),
            Some(b.account_id.as_str())
        );
        let active = repo.load_active().unwrap().unwrap();
        assert_eq!(active.username, "Bob");

        assert_eq!(repo.list_sessions().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remove_selected_falls_back() {
        let dir = temp_dir("remove");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let repo = AccountRepository::new(&dir, vault).unwrap();

        let a = session("offline", "Alice");
        let b = session("offline", "Bob");
        repo.save(&a).unwrap();
        repo.save(&b).unwrap();
        repo.select(&a.account_id).unwrap();
        repo.remove(&a.account_id).unwrap();

        assert_eq!(
            repo.selected_account_id().unwrap().as_deref(),
            Some(b.account_id.as_str())
        );
        assert!(repo.load(&a.account_id).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_metadata_is_plaintext_no_tokens() {
        let dir = temp_dir("plain");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let repo = AccountRepository::new(&dir, vault).unwrap();

        let a = session("microsoft", "Alice");
        repo.save(&a).unwrap();

        let raw = std::fs::read_to_string(repo.metadata_path()).unwrap();
        assert!(
            raw.contains("Alice"),
            "metadata must be readable as plaintext"
        );
        assert!(
            !raw.contains("token-Alice"),
            "metadata must NOT contain credentials"
        );

        let raw_private = std::fs::read_to_string(repo.private_path()).unwrap();
        assert!(
            !raw_private.contains("token-Alice"),
            "private data must be encrypted at rest"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── P2: account lifecycle via factories ──

    #[test]
    fn test_save_account_load_account_offline() {
        let dir = temp_dir("account-roundtrip");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let mut repo = AccountRepository::new(&dir, vault).unwrap();
        repo.register_factory(Box::new(crate::offline::OfflineAccountFactory));

        let account = crate::offline::OfflineAccount::new("Steve");
        repo.save_account(&account).unwrap();

        let restored = repo
            .load_account(&account.account_id().to_string())
            .unwrap()
            .expect("account must be restored from storage");
        assert_eq!(restored.account_id(), account.account_id());
        assert_eq!(restored.provider(), "offline");
        assert_eq!(restored.profile_name(), "Steve");

        // 恢复后可静默 log_in（离线 → msa + 随机 token）。
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let info = runtime.block_on(restored.log_in()).unwrap();
        assert_eq!(info.username, "Steve");
        assert_eq!(info.user_type, crate::auth_info::user_type::MSA);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_active_account_returns_selected() {
        let dir = temp_dir("active-account");
        let vault = Arc::new(TokenVault::init(&dir.join(".key")).unwrap());
        let mut repo = AccountRepository::new(&dir, vault).unwrap();
        repo.register_factory(Box::new(crate::offline::OfflineAccountFactory));

        let a = crate::offline::OfflineAccount::new("Alice");
        let b = crate::offline::OfflineAccount::new("Bob");
        repo.save_account(&a).unwrap();
        repo.save_account(&b).unwrap();
        repo.select(&b.account_id().to_string()).unwrap();

        let active = repo
            .load_active_account()
            .unwrap()
            .expect("active account must be restored");
        assert_eq!(active.profile_name(), "Bob");

        // 未注册工厂的 provider → 明确报错而非静默返回 None。
        let bad = session("microsoft", "Carol");
        repo.save(&bad).unwrap();
        assert!(
            repo.load_account(&bad.account_id).is_err(),
            "missing factory must surface an error"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
