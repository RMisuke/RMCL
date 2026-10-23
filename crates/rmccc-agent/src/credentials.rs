//! Encrypted credential storage for model providers (ADR-009).
//!
//! # 威胁模型
//!
//! 与账号令牌一致：攻击者能读用户目录时也能读 key 文件，因此这是"同机同用户"边界，
//! 不是 OS 密钥链级保护。收益是纯 Rust、可测试、无平台分支；系统密钥链留作
//! [`CredentialStore`] 的替代后端（ADR-009 选项 A）。
//!
//! # 不变量
//!
//! 1. `api_key` 只在 Rust 侧解密后使用，公开类型 [`CredentialSummary`] 不含它。
//! 2. 磁盘文件是 [`rmccc_auth::TokenVault`] 的版本化信封，明文 key 不会落盘。
//! 3. 远程端点必须 HTTPS；明文只允许 loopback（本地模型）。

use rmccc_auth::TokenVault;
use rmccc_error::{Result, RmcccError};
use rmccc_platform::PlatformFs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use url::{Host, Url};

/// 凭据文件名（`<data_dir>/private/agent-credentials.json`）。
pub const CREDENTIALS_FILE: &str = "agent-credentials.json";
/// 加密 key 文件名（`<data_dir>/private/agent.key`）。
pub const KEY_FILE: &str = "agent.key";

const ENVELOPE_VERSION: u32 = 1;

/// 单个服务的凭据。`Debug` 对 `api_key` 打码，避免日志/`dbg!` 泄露。
///
/// 手写 `Debug` 而不是 `derive`：这是唯一能保证"任何日志路径都不会打印出 key"的地方。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceCredential {
    pub service: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl fmt::Debug for ServiceCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceCredential")
            .field("service", &self.service)
            .field("api_key", &"«redacted»")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl ServiceCredential {
    /// 新建一条凭据。`service` 只允许 `[a-z0-9._-]`，避免被当作路径或日志注入载体。
    pub fn new(service: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let service = service.into();
        validate_service(&service)?;
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(RmcccError::Validation(
                "credential api_key must not be empty".into(),
            ));
        }
        Ok(Self {
            service,
            api_key,
            base_url: None,
            model: None,
        })
    }

    /// 设置端点；构造即校验（HTTPS 或 loopback HTTP，见 [`validate_endpoint`]）。
    pub fn with_endpoint(mut self, base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into();
        validate_endpoint(&base_url)?;
        self.base_url = Some(base_url);
        Ok(self)
    }

    /// 设置模型 id。
    pub fn with_model(mut self, model: impl Into<String>) -> Result<Self> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err(RmcccError::Validation("model id must not be empty".into()));
        }
        self.model = Some(model);
        Ok(self)
    }
}

/// 非秘密视图：供设置页显示"是否已保存"，永远不含 `api_key`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialSummary {
    pub service: String,
    pub has_key: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl From<&ServiceCredential> for CredentialSummary {
    fn from(credential: &ServiceCredential) -> Self {
        Self {
            service: credential.service.clone(),
            has_key: !credential.api_key.is_empty(),
            base_url: credential.base_url.clone(),
            model: credential.model.clone(),
        }
    }
}

/// 凭据存储的唯一入口（ADR-009）。系统密钥链后端实现同一 trait 即可替换。
pub trait CredentialStore: Send + Sync {
    fn put(&self, credential: ServiceCredential) -> Result<()>;
    fn get(&self, service: &str) -> Result<Option<ServiceCredential>>;
    fn remove(&self, service: &str) -> Result<bool>;
    fn list(&self) -> Result<Vec<CredentialSummary>>;
}

/// AES-256-GCM 文件 vault（`<data_dir>/private/agent-credentials.json`）。
///
/// 与账号令牌使用**不同**的 key 文件（ADR-009）：一次泄露不应同时波及游戏账号与模型账单。
pub struct FileCredentialStore {
    credentials_path: PathBuf,
    vault: TokenVault,
    fs: Arc<dyn PlatformFs>,
    /// 串行化「读-改-写」，避免两个并发 `put` 丢条目。
    write_lock: Mutex<()>,
}

impl FileCredentialStore {
    /// 打开（必要时创建）`<data_dir>/private/` 下的 vault。
    pub fn open(data_dir: &Path, fs: Arc<dyn PlatformFs>) -> Result<Self> {
        let private_dir = data_dir.join("private");
        std::fs::create_dir_all(&private_dir)?;
        let key_path = private_dir.join(KEY_FILE);
        let vault = TokenVault::init(&key_path, fs.as_ref())?;
        Ok(Self {
            credentials_path: private_dir.join(CREDENTIALS_FILE),
            vault,
            fs,
            write_lock: Mutex::new(()),
        })
    }

    /// vault 文件路径（诊断/备份用；不含秘密）。
    pub fn path(&self) -> &Path {
        &self.credentials_path
    }

    fn load(&self) -> Result<CredentialFile> {
        if !self.credentials_path.exists() {
            return Ok(CredentialFile::default());
        }
        let raw = std::fs::read_to_string(&self.credentials_path)?;
        let file: CredentialFile = self.vault.decrypt_payload(&raw)?;
        if file.version != ENVELOPE_VERSION {
            return Err(RmcccError::Validation(format!(
                "unsupported credential vault version: {} (expected {ENVELOPE_VERSION})",
                file.version
            )));
        }
        Ok(file)
    }

    /// 原子替换 + 收紧权限；失败时旧文件保留（`PlatformFs::atomic_replace` 语义）。
    fn store(&self, file: &CredentialFile) -> Result<()> {
        let envelope = self.vault.encrypt_payload(file)?;
        let dir = self
            .credentials_path
            .parent()
            .ok_or_else(|| RmcccError::Other("credential path has no parent".into()))?;
        let tmp = dir.join(format!("{CREDENTIALS_FILE}.tmp"));
        std::fs::write(&tmp, envelope.as_bytes())?;
        self.fs.restrict_owner_only(&tmp)?;
        self.fs.atomic_replace(&tmp, &self.credentials_path)?;
        // 目标可能由 rename 保留旧权限；再显式收紧一次。
        self.fs.restrict_owner_only(&self.credentials_path)?;
        Ok(())
    }
}

impl CredentialStore for FileCredentialStore {
    fn put(&self, credential: ServiceCredential) -> Result<()> {
        validate_service(&credential.service)?;
        if credential.api_key.trim().is_empty() {
            return Err(RmcccError::Validation(
                "credential api_key must not be empty".into(),
            ));
        }
        if let Some(ref base_url) = credential.base_url {
            validate_endpoint(base_url)?;
        }
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("credential store lock poisoned".into()))?;
        let mut file = self.load()?;
        file.credentials
            .insert(credential.service.clone(), credential);
        file.version = ENVELOPE_VERSION;
        self.store(&file)
    }

    fn get(&self, service: &str) -> Result<Option<ServiceCredential>> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("credential store lock poisoned".into()))?;
        Ok(self.load()?.credentials.remove(service))
    }

    fn remove(&self, service: &str) -> Result<bool> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("credential store lock poisoned".into()))?;
        let mut file = self.load()?;
        let removed = file.credentials.remove(service).is_some();
        if removed {
            self.store(&file)?;
        }
        Ok(removed)
    }

    fn list(&self) -> Result<Vec<CredentialSummary>> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RmcccError::Other("credential store lock poisoned".into()))?;
        let file = self.load()?;
        Ok(file
            .credentials
            .values()
            .map(CredentialSummary::from)
            .collect())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    credentials: BTreeMap<String, ServiceCredential>,
}

impl Default for CredentialFile {
    fn default() -> Self {
        Self {
            version: ENVELOPE_VERSION,
            credentials: BTreeMap::new(),
        }
    }
}

fn default_version() -> u32 {
    ENVELOPE_VERSION
}

fn validate_service(service: &str) -> Result<()> {
    if service.is_empty() || service.len() > 64 {
        return Err(RmcccError::Validation(
            "credential service must be 1..=64 characters".into(),
        ));
    }
    if !service
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err(RmcccError::Validation(format!(
            "credential service '{service}' must match [a-z0-9._-]"
        )));
    }
    Ok(())
}

/// 端点策略（ADR-009 决策 5）：远程必须 HTTPS，明文只允许 loopback。
///
/// 校验放在这里而不是 UI：任何调用方（CLI、插件授权代发、未来 Agent loop）都必须过同一道门。
pub fn validate_endpoint(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)
        .map_err(|e| RmcccError::Validation(format!("invalid endpoint '{raw}': {e}")))?;
    match url.scheme() {
        "https" => Ok(url),
        "http" => {
            let loopback = match url.host() {
                Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(Host::Ipv4(ip)) => ip.is_loopback(),
                Some(Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            };
            if loopback {
                Ok(url)
            } else {
                Err(RmcccError::Validation(format!(
                    "plaintext HTTP is only allowed for loopback endpoints, got '{raw}'"
                )))
            }
        }
        other => Err(RmcccError::Validation(format!(
            "unsupported endpoint scheme '{other}' (expected https or loopback http)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::Platform;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "rmccc-agent-cred-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn store(tag: &str) -> (PathBuf, FileCredentialStore) {
        let dir = temp_dir(tag);
        let store = FileCredentialStore::open(&dir, Platform::current().fs.clone())
            .expect("credential store");
        (dir, store)
    }

    #[test]
    fn roundtrip_and_summary_hide_the_key() {
        let (dir, store) = store("roundtrip");
        let credential = ServiceCredential::new("openai", "sk-test-1234567890")
            .expect("cred")
            .with_endpoint("https://api.openai.com/v1")
            .expect("endpoint")
            .with_model("gpt-4o-mini")
            .expect("model");
        store.put(credential.clone()).expect("put");

        let loaded = store.get("openai").expect("get").expect("present");
        assert_eq!(loaded, credential);

        let listed = store.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].service, "openai");
        assert!(listed[0].has_key);
        // 公开视图没有 api_key 字段可以泄露。
        let json = serde_json::to_string(&listed).expect("json");
        assert!(!json.contains("sk-test-1234567890"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_on_disk_is_encrypted_and_lacks_plaintext() {
        let (dir, store) = store("encrypted");
        store
            .put(ServiceCredential::new("openai", "sk-super-secret-value").expect("cred"))
            .expect("put");
        let raw = std::fs::read_to_string(store.path()).expect("vault file");
        assert!(!raw.contains("sk-super-secret-value"));
        assert!(
            !raw.contains("openai\""),
            "service names are inside the envelope"
        );
        let envelope: serde_json::Value = serde_json::from_str(&raw).expect("envelope json");
        assert_eq!(envelope["version"], 1);
        assert!(envelope["ciphertext"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn debug_redacts_api_key() {
        let credential = ServiceCredential::new("local", "sk-very-secret").expect("cred");
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("sk-very-secret"));
        assert!(rendered.contains("«redacted»"));
    }

    #[test]
    fn remove_is_idempotent() {
        let (dir, store) = store("remove");
        store
            .put(ServiceCredential::new("openai", "sk-a").expect("cred"))
            .expect("put");
        assert!(store.remove("openai").expect("remove"));
        assert!(!store.remove("openai").expect("remove again"));
        assert!(store.list().expect("list").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn service_names_are_validated() {
        assert!(ServiceCredential::new("Open AI", "sk-a").is_err());
        assert!(ServiceCredential::new("../escape", "sk-a").is_err());
        assert!(ServiceCredential::new("", "sk-a").is_err());
        assert!(ServiceCredential::new("openai", "  ").is_err());
        assert!(ServiceCredential::new("local-llm_2", "sk-a").is_ok());
    }

    #[test]
    fn endpoint_policy_allows_https_and_loopback_only() {
        assert!(validate_endpoint("https://api.openai.com/v1").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_endpoint("http://localhost:8080/v1").is_ok());
        assert!(validate_endpoint("http://[::1]:8080/v1").is_ok());
        assert!(validate_endpoint("http://example.com/v1").is_err());
        assert!(validate_endpoint("http://192.168.1.10:8080").is_err());
        assert!(validate_endpoint("file:///etc/passwd").is_err());
        assert!(validate_endpoint("api.openai.com").is_err());
    }

    #[test]
    fn put_rejects_endpoint_violation_even_if_constructed_manually() {
        let (dir, store) = store("policy");
        let mut credential = ServiceCredential::new("openai", "sk-a").expect("cred");
        credential.base_url = Some("http://evil.example.com".into());
        assert!(store.put(credential).is_err());
        assert!(store.list().expect("list").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_key_file_cannot_decrypt() {
        let (dir, store) = store("wrong-key");
        store
            .put(ServiceCredential::new("openai", "sk-a").expect("cred"))
            .expect("put");
        // 同一个 vault 实例已经把旧 key 载入内存，必须重开才能验证"key 被换掉"。
        drop(store);
        std::fs::write(dir.join("private/agent.key"), [7u8; 32]).expect("replace key");
        let reopened =
            FileCredentialStore::open(&dir, Platform::current().fs.clone()).expect("reopen");
        assert!(reopened.get("openai").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
