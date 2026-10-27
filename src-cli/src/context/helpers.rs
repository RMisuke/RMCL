//! 域无关的工具函数（路径、解析、SHA1 校验）与 CLI 侧单测。
//!
//! 拆分前它们与 `CliContext` 挤在同一个 1,450 行文件里；现在独立成模块，
//! 只暴露给 `context` 及其子模块（`pub(super)`）。

use super::*;

// ────────────────────────── Helpers ──────────────────────────

/// Process listener that syncs CLI state (running games + exit signals).
pub(super) struct CliProcessListener {
    pub(super) key: String,
    pub(super) running_games: Arc<dashmap::DashMap<String, u32>>,
    pub(super) exit_tx: tokio::sync::watch::Sender<Option<i32>>,
    pub(super) output: Option<GameOutputCb>,
    pub(super) on_exit: Option<CliExitCb>,
}

impl rmccc_launch::ProcessListener for CliProcessListener {
    fn on_log(&self, line: String, _is_stderr: bool) {
        if let Some(cb) = &self.output {
            cb(&line);
        }
    }
    fn on_exit(&self, exit_code: i32, exit_type: rmccc_launch::ExitType) {
        self.running_games.remove(&self.key);
        let _ = self.exit_tx.send(Some(exit_code));
        if let Some(cb) = &self.on_exit {
            cb(exit_code, exit_type);
        }
    }
}

pub(super) fn parse_bounded_u32(value: &str, min: u32, max: u32) -> Result<u32> {
    let v: u32 = value
        .parse()
        .map_err(|_| RmcccError::Other(format!("'{value}' is not a valid u32")))?;
    Ok(v.clamp(min, max))
}

pub(super) fn parse_u64_field(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| RmcccError::Other(format!("'{value}' is not a valid u64")))
}

pub(super) fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(RmcccError::Other(format!(
            "'{other}' is not a valid boolean"
        ))),
    }
}

pub(super) fn parse_optional_str(value: &str) -> Result<Option<String>> {
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

/// Sanitize a filename: keep ASCII alphanumerics and `-_.`, replace the rest.
pub(super) fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Reject path traversal / invalid characters in user-supplied filenames.
pub(super) fn validate_mod_filename(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(RmcccError::Other(format!("Invalid mod filename: {name}")));
    }
    Ok(())
}

/// Verify a file's SHA1 against an expected hash.
pub(super) fn verify_sha1(path: &Path, expected: &str) -> std::io::Result<bool> {
    use sha1::{Digest, Sha1};
    let data = std::fs::read(path)?;
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual.to_lowercase() == expected.to_lowercase())
}

/// Convert a Maven coordinate into a library file path.
pub(super) fn make_library_path(name: &str, libs_dir: &Path) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(name);
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    libs_dir.join(format!(
        "{}/{}/{}/{}-{}.jar",
        group, artifact, version, artifact, version
    ))
}

/// Top-level entry names of a directory.
pub(super) fn walk_dir_top(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rmccc-cli-test-{tag}-{}-{}",
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

    async fn seed_installed_version(ctx: &CliContext, version: &str) {
        let root = {
            let core = ctx.core.read().await;
            let service = core.instances.lock().unwrap();
            let path = service.selected_directory().path.clone();
            drop(service);
            path
        };
        let dir = root.join("versions").join(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{version}.json")),
            format!(
                r#"{{"id":"{version}","type":"release","mainClass":"net.minecraft.client.main.Main","assetIndex":{{"id":"","url":"","sha1":"","size":0,"totalSize":0}},"downloads":{{}},"libraries":[],"javaVersion":null}}"#
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn profile_crud_roundtrip() {
        let dir = temp_dir("profile");
        let ctx = CliContext::new(&dir).unwrap();

        seed_installed_version(&ctx, "1.21").await;
        let created = ctx
            .profile_create("My Instance", "1.21", Some("fabric"), Some(2048))
            .await
            .unwrap();
        assert_eq!(created.summary.display_name, "My Instance");
        assert_eq!(created.effective.max_memory_mb, 2048);

        let listed = ctx.profile_list().await.unwrap();
        assert_eq!(listed.len(), 1);

        let updated = ctx
            .profile_update(
                &created.summary.key,
                Some("Renamed"),
                None,
                None,
                None,
                None,
                Some(4096),
                None,
            )
            .await
            .unwrap();
        assert_eq!(updated.summary.display_name, "Renamed");
        assert_eq!(updated.effective.max_memory_mb, 4096);

        ctx.profile_delete(&created.summary.key, false)
            .await
            .unwrap();
        assert!(ctx.profile_list().await.unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn profile_create_validates_name() {
        let dir = temp_dir("validate");
        let ctx = CliContext::new(&dir).unwrap();
        assert!(ctx.profile_create("", "1.21", None, None).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn config_set_get_roundtrip() {
        let dir = temp_dir("config");
        let ctx = CliContext::new(&dir).unwrap();
        ctx.config_set("default_max_memory_mb", "8192")
            .await
            .unwrap();
        let v = ctx.config_get(Some("default_max_memory_mb")).await.unwrap();
        assert_eq!(v, json!(8192));
        assert!(ctx.config_set("no_such_field", "1").await.is_err());
        assert!(ctx
            .config_set("default_max_memory_mb", "abc")
            .await
            .is_err());

        let paths = ctx.config_paths().await.unwrap();
        assert_eq!(
            paths["game_root_dir"],
            dir.join("instances").display().to_string()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auth_offline_login_and_logout() {
        let dir = temp_dir("auth");
        let ctx = CliContext::new(&dir).unwrap();
        let session = ctx.auth_login_offline("Steve").await.unwrap();
        assert_eq!(session.username, "Steve");

        let status = ctx.auth_status().await.unwrap();
        assert_eq!(status.as_ref().map(|s| s.username.as_str()), Some("Steve"));

        let list = ctx.auth_list().await.unwrap();
        assert_eq!(list.len(), 1);

        ctx.auth_logout().await.unwrap();
        assert!(ctx.auth_status().await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn state_and_user_settings() {
        let dir = temp_dir("state");
        let ctx = CliContext::new(&dir).unwrap();
        seed_installed_version(&ctx, "1.21").await;
        let instance = ctx.profile_create("p", "1.21", None, None).await.unwrap();
        ctx.profile_select(&instance.summary.key).await.unwrap();
        let state = ctx.state_get(None).await.unwrap();
        assert_eq!(state["selected_key"], instance.summary.key);

        ctx.user_set("locale", "zh-CN").await.unwrap();
        let user = ctx.user_get(Some("locale")).await.unwrap();
        assert_eq!(user, "zh-CN");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filename_validation_blocks_traversal() {
        assert!(validate_mod_filename("../evil.jar").is_err());
        assert!(validate_mod_filename("a/b.jar").is_err());
        assert!(validate_mod_filename("ok-mod-1.0.jar").is_ok());
        assert!(validate_mod_filename("").is_err());
    }
}
