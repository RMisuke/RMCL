//! CLI context — wraps backend API for command handlers.
//!
//! This is the facade that both CLI and Tauri commands will use.
//! It holds all providers and exposes a clean, unified API.

use rmccc_auth::AuthProvider;
use rmccc_core::CoreContext;
use rmccc_error::Result;
use std::path::Path;
use std::sync::Arc;

/// Unified backend API facade used by CLI and Tauri.
#[derive(Clone)]
pub struct CliContext {
    pub core: Arc<tokio::sync::RwLock<CoreContext>>,
    pub http_client: reqwest::Client,
}

impl CliContext {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let core = CoreContext::new(data_dir)?;
        let http_client = core.http_client.clone();
        Ok(Self { core: Arc::new(tokio::sync::RwLock::new(core)), http_client })
    }

    // ── helpers ──

    async fn data_dir(&self) -> std::path::PathBuf {
        self.core.read().await.data_dir.clone()
    }

    // ── Auth ──

    pub async fn auth_login_microsoft(&self) -> Result<rmccc_auth::AuthSession> {
        let provider = rmccc_auth::MicrosoftAuthProvider::new(self.http_client.clone());
        let session = provider.authenticate(None).await?;
        let dir = self.data_dir().await;
        let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
        vault.save_session(&session, &dir.join("auth.json"))?;
        Ok(session)
    }

    pub async fn auth_login_offline(&self, username: &str) -> Result<rmccc_auth::AuthSession> {
        let session = rmccc_auth::create_offline_session(username);
        let dir = self.data_dir().await;
        let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
        vault.save_session(&session, &dir.join("auth.json"))?;
        Ok(session)
    }

    pub async fn auth_logout(&self) -> Result<()> {
        let dir = self.data_dir().await;
        let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
        vault.clear_session(&dir.join("auth.json"))?;
        Ok(())
    }

    pub async fn auth_refresh(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let dir = self.data_dir().await;
        let auth_path = dir.join("auth.json");
        let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
        let session = vault.load_session(&auth_path)?;
        match session {
            Some(s) if s.provider == "microsoft" && s.refresh_token.is_some() => {
                let expired = s.expires_at.map(|e| chrono::Utc::now() >= e).unwrap_or(false);
                if expired {
                    let provider = rmccc_auth::MicrosoftAuthProvider::new(self.http_client.clone());
                    match provider.refresh(&s).await {
                        Ok(new) => { vault.save_session(&new, &auth_path)?; Ok(Some(new)) }
                        Err(e) => { tracing::warn!("Token refresh failed: {e}"); Ok(Some(s)) }
                    }
                } else { Ok(Some(s)) }
            }
            Some(s) => Ok(Some(s)),
            None => Ok(None),
        }
    }

    pub async fn auth_get_session(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let dir = self.data_dir().await;
        let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
        vault.load_session(&dir.join("auth.json"))
    }

    // ── Instance ──

    pub async fn instance_list(&self) -> Result<Vec<rmccc_profile::Profile>> {
        self.core.read().await.profile_store.list()
    }

    pub async fn instance_create(&self, name: &str, game_version: Option<&str>, loader: Option<&str>) -> Result<rmccc_profile::Profile> {
        let core = self.core.read().await;
        let version = game_version.map(|v| v.to_string()).unwrap_or_else(|| "1.20.4".to_string());
        let mod_loader = loader.map(|l| rmccc_profile::ModLoaderConfig { loader_type: l.to_string(), loader_version: String::new() });
        let game_dir = core.config.profile_game_dir(name);
        let profile = rmccc_profile::Profile {
            id: uuid::Uuid::new_v4().to_string(), name: name.to_string(), game_version: version,
            mod_loader, java_path: None, jvm_args: vec![], game_args: vec![],
            memory_mb: core.config.default_max_memory_mb, game_dir,
            created_at: chrono::Utc::now(), last_played: None, icon: None,
        };
        core.profile_store.save(&profile)?;
        Ok(profile)
    }

    pub async fn instance_delete(&self, id: &str, cleanup_files: bool) -> Result<()> {
        self.core.read().await.profile_store.delete(id, cleanup_files)
    }

    // ── Resource download ──

    pub async fn resource_download(&self, instance_id: &str, version_id: &str) -> Result<()> {
        let core = self.core.read().await;
        let profile = core.profile_store.get(instance_id)?
            .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(instance_id.to_string()))?;

        // Resolve version: prefer local patched JSON (e.g. Fabric/Forge/NeoForge),
        // handle inheritsFrom chains by merging with Mojang API data
        let local_json_path = profile.game_dir.join("versions").join(version_id).join(format!("{version_id}.json"));
        let version = if local_json_path.exists() {
            let content = std::fs::read_to_string(&local_json_path)
                .map_err(|e| rmccc_error::RmcccError::Other(format!("读取本地版本 JSON 失败: {e}")))?;

            // Try direct parse first (e.g. Fabric patched JSON is complete)
            if let Ok(v) = serde_json::from_str::<rmccc_version::GameVersion>(&content) {
                v
            } else {
                // Handle inheritsFrom-based versions (NeoForge/Forge): parse as Value
                // to extract inheritsFrom, then resolve via Mojang API
                let raw: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| rmccc_error::RmcccError::Other(format!("解析本地版本 JSON 失败: {e}")))?;

                if let Some(parent_id) = raw.get("inheritsFrom").and_then(|v| v.as_str()) {
                    let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
                    let mut resolved = rmccc_version::resolve_version(parent_id, &provider).await?;

                    // Apply local overrides: mainClass, libraries, arguments
                    if let Some(mc) = raw.get("mainClass").and_then(|v| v.as_str()) {
                        resolved.main_class = mc.to_string();
                    }
                    if let Some(libs) = raw.get("libraries").and_then(|v| v.as_array()) {
                        let local_libs: Vec<rmccc_version::Library> =
                            serde_json::from_value(serde_json::Value::Array(libs.clone()))
                                .unwrap_or_default();
                        for lib in local_libs {
                            if !resolved.libraries.iter().any(|l| l.name == lib.name) {
                                resolved.libraries.push(lib);
                            }
                        }
                    }
                    if let Some(args) = raw.get("arguments") {
                        if let Ok(a) = serde_json::from_value(args.clone()) {
                            resolved.arguments = Some(a);
                        }
                    }
                    resolved.id = version_id.to_string();
                    resolved
                } else {
                    // No inheritsFrom and direct parse failed — fall through to Mojang API
                    let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
                    rmccc_version::resolve_version(version_id, &provider).await?
                }
            }
        } else {
            let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
            rmccc_version::resolve_version(version_id, &provider).await?
        };

        // Use inject_url_with_candidates for multi-mirror fallback
        let inject_candidates = |url: &str| -> Vec<String> {
            core.download_provider.inject_url_with_candidates(url)
        };
        for subdir in &["versions", "libraries", "assets"] {
            std::fs::create_dir_all(profile.game_dir.join(subdir))?;
        }

        // ── Step 1: Download client JAR ──
        if let Some(client_dl) = version.downloads.get("client") {
            let jar_path = profile.game_dir.join("versions").join(version_id).join(format!("{}.jar", version_id));
            std::fs::create_dir_all(jar_path.parent().unwrap())?;
            let candidates = inject_candidates(&client_dl.url);
            let items = vec![rmccc_download::DownloadItem {
                url: candidates[0].clone(),
                dest: jar_path,
                expected_sha1: Some(client_dl.sha1.clone()),
                size: Some(client_dl.size),
                use_cache: true,
                candidates: candidates[1..].to_vec(),
            }];
            eprint!("\r\x1b[K下载客户端 JAR...");
            let handle = core.download_manager.download_batch(items, progress_cb("JAR")).await?;
            core.download_manager.wait_for_batch(&handle.id).await;
            eprintln!("\r\x1b[K✅ 客户端 JAR 下载完成");

            // Save version JSON to versions/{id}/{id}.json (only if it doesn't exist yet,
            // to avoid overwriting patched versions from modloader install)
            let version_json_path = profile.game_dir.join("versions").join(version_id).join(format!("{}.json", version_id));
            if !version_json_path.exists() {
                if let Some(ref raw_json) = version.raw_json {
                    std::fs::write(&version_json_path, raw_json)?;
                }
            }
        }

        // ── Step 2: Download libraries ──
        let libs_dir = profile.game_dir.join("libraries");
        let lib_items: Vec<rmccc_download::DownloadItem> = rmccc_version::libraries::resolve_library_items(&version.libraries, &libs_dir)
            .into_iter().map(|mut item| {
                let candidates = inject_candidates(&item.url);
                item.url = candidates[0].clone();
                item.candidates = candidates[1..].to_vec();
                item
            }).collect();
        let lib_count = lib_items.len();
        if !lib_items.is_empty() {
            eprint!("\r\x1b[K下载依赖库 ({} 个)...", lib_count);
            let handle = core.download_manager.download_batch(lib_items, progress_cb("依赖库")).await?;
            core.download_manager.wait_for_batch(&handle.id).await;
            eprintln!("\r\x1b[K✅ 依赖库下载完成 ({} 个)", lib_count);
        }

        // ── Step 3: Download assets ──
        let asset_items: Vec<rmccc_download::DownloadItem> = rmccc_version::assets::resolve_asset_items(
            &self.http_client, &version.asset_index, &profile.game_dir.join("assets"),
        ).await?.into_iter().map(|mut item| {
            let candidates = inject_candidates(&item.url);
            item.url = candidates[0].clone();
            item.candidates = candidates[1..].to_vec();
            item
        }).collect();
        let asset_count = asset_items.len();
        if !asset_items.is_empty() {
            eprint!("\r\x1b[K下载资源文件 ({} 个)...", asset_count);
            let handle = core.download_manager.download_batch(asset_items, progress_cb(&format!("资源 ({})", asset_count))).await?;
            core.download_manager.wait_for_batch(&handle.id).await;
            eprintln!("\r\x1b[K✅ 资源文件下载完成 ({} 个)", asset_count);
        }

        // ── Step 4: Extract native libraries ──
        let natives_dir = profile.game_dir.join("versions").join(version_id).join("natives");
        eprint!("\r\x1b[K解压 Native 库...");
        let natives_count = rmccc_launch::decompress_natives(
            &version.libraries,
            &libs_dir,
            &natives_dir,
            false,
            false,
        )?;
        if natives_count > 0 {
            eprintln!("\r\x1b[K✅ Native 库解压完成 ({} 个文件)", natives_count);
        } else {
            eprintln!("\r\x1b[K✅ Native 库检查完成 (无需解压)");
        }

        tracing::info!("Version {} installed for instance '{}'", version_id, profile.name);
        Ok(())
    }

    // ── Launch ──

    /// Resolve a game version, handling local patched JSON with inheritsFrom.
    async fn resolve_game_version(&self, profile: &rmccc_profile::Profile) -> Result<rmccc_version::GameVersion> {
        let local_json_path = profile.game_dir.join("versions")
            .join(&profile.game_version)
            .join(format!("{}.json", profile.game_version));

        if local_json_path.exists() {
            let content = std::fs::read_to_string(&local_json_path)
                .map_err(|e| rmccc_error::RmcccError::Other(format!("读取本地版本 JSON 失败: {e}")))?;

            // Try direct parse first (e.g. Fabric patched JSON is complete)
            if let Ok(v) = serde_json::from_str::<rmccc_version::GameVersion>(&content) {
                return Ok(v);
            }

            // Handle inheritsFrom-based versions (NeoForge/Forge): resolve base + merge overrides
            let raw: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| rmccc_error::RmcccError::Other(format!("解析本地版本 JSON 失败: {e}")))?;

            if let Some(parent_id) = raw.get("inheritsFrom").and_then(|v| v.as_str()) {
                let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
                let mut resolved = rmccc_version::resolve_version(parent_id, &provider).await?;

                if let Some(mc) = raw.get("mainClass").and_then(|v| v.as_str()) {
                    resolved.main_class = mc.to_string();
                }
                if let Some(libs) = raw.get("libraries").and_then(|v| v.as_array()) {
                    let local_libs: Vec<rmccc_version::Library> =
                        serde_json::from_value(serde_json::Value::Array(libs.clone()))
                            .unwrap_or_default();
                    for lib in local_libs {
                        if !resolved.libraries.iter().any(|l| l.name == lib.name) {
                            resolved.libraries.push(lib);
                        }
                    }
                }
                if let Some(args) = raw.get("arguments") {
                    if let Ok(a) = serde_json::from_value(args.clone()) {
                        resolved.arguments = Some(a);
                    }
                }
                resolved.id = profile.game_version.clone();
                return Ok(resolved);
            }
        }

        // Fall back to Mojang API
        let provider = rmccc_version::MojangVersionProvider::new(self.http_client.clone());
        rmccc_version::resolve_version(&profile.game_version, &provider).await
    }

    /// Verify and repair missing resources before launching.
    ///
    /// Checks for critical files and auto-downloads any that are missing:
    /// - Client JAR (versions/{id}/{id}.jar)
    /// - Version JSON (versions/{id}/{id}.json)
    /// - Asset index (assets/indexes/{asset_index_id}.json)
    /// - Library JARs (libraries/...)
    ///
    /// Uses the existing download infrastructure with `wait_for_batch` to
    /// ensure files are fully written before proceeding.
    async fn repair_resources(&self, profile: &rmccc_profile::Profile) -> Result<bool> {
        let core = self.core.read().await;
        let version = self.resolve_game_version(profile).await?;

        let inject_candidates = |url: &str| -> Vec<String> {
            core.download_provider.inject_url_with_candidates(url)
        };

        let mut repaired = false;
        let version_dir = profile.game_dir.join("versions").join(&profile.game_version);

        // Check client JAR
        let jar_path = version_dir.join(format!("{}.jar", profile.game_version));
        let jar_needs_repair = !jar_path.exists() || std::fs::metadata(&jar_path).map(|m| m.len()).unwrap_or(0) == 0;
        if jar_needs_repair {
            if let Some(client_dl) = version.downloads.get("client") {
                std::fs::create_dir_all(jar_path.parent().unwrap())?;
                let candidates = inject_candidates(&client_dl.url);
                let items = vec![rmccc_download::DownloadItem {
                    url: candidates[0].clone(),
                    dest: jar_path,
                    expected_sha1: Some(client_dl.sha1.clone()),
                    size: Some(client_dl.size),
                    use_cache: true,
                    candidates: candidates[1..].to_vec(),
                }];
                eprint!("\r\x1b[K修复客户端 JAR...");
                let handle = core.download_manager.download_batch(items, progress_cb("JAR")).await?;
                core.download_manager.wait_for_batch(&handle.id).await;
                eprintln!("\r\x1b[K✅ 客户端 JAR 已修复");
                repaired = true;
            }
        }

        // Check version JSON
        let json_path = version_dir.join(format!("{}.json", profile.game_version));
        if !json_path.exists() {
            if let Some(ref raw_json) = version.raw_json {
                std::fs::create_dir_all(json_path.parent().unwrap())?;
                std::fs::write(&json_path, raw_json)?;
                eprintln!("✅ 版本 JSON 已写入");
                repaired = true;
            }
        }

        // Check asset index
        let index_path = profile.game_dir.join("assets").join("indexes")
            .join(format!("{}.json", version.asset_index.id));
        if !index_path.exists() {
            // Re-download the asset index
            let resp = self.http_client
                .get(&version.asset_index.url)
                .send().await
                .map_err(|e| rmccc_error::RmcccError::Network(e.to_string()))?;
            let body = resp.bytes().await
                .map_err(|e| rmccc_error::RmcccError::Network(e.to_string()))?;
            std::fs::create_dir_all(index_path.parent().unwrap())?;
            std::fs::write(&index_path, &body)?;
            eprintln!("✅ 资源索引已修复: {}.json", version.asset_index.id);
            repaired = true;
        }

        // Check library files (quick check: verify at least the first 5 exist)
        let libs_dir = profile.game_dir.join("libraries");
        let lib_items = rmccc_version::libraries::resolve_library_items(&version.libraries, &libs_dir);
        let missing_libs: Vec<_> = lib_items.into_iter()
            .filter(|item| !item.dest.exists())
            .collect();
        if !missing_libs.is_empty() {
            let count = missing_libs.len();
            eprint!("\r\x1b[K修复依赖库 ({} 个缺失)...", count);
            let items: Vec<_> = missing_libs.into_iter().map(|mut item| {
                let candidates = inject_candidates(&item.url);
                item.url = candidates[0].clone();
                item.candidates = candidates[1..].to_vec();
                item
            }).collect();
            let handle = core.download_manager.download_batch(items, progress_cb("依赖库修复")).await?;
            core.download_manager.wait_for_batch(&handle.id).await;
            eprintln!("\r\x1b[K✅ 依赖库已修复 ({} 个)", count);
            repaired = true;
        }

        Ok(repaired)
    }

    pub async fn launch_run(&self, instance_id: &str) -> Result<()> {
        let core = self.core.read().await;
        let profile = core.profile_store.get(instance_id)?
            .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(instance_id.to_string()))?;

        // ── Pre-launch resource integrity check & auto-repair ──
        drop(core); // release lock before async repair
        let repaired = self.repair_resources(&profile).await?;
        if repaired {
            println!("🔧 资源已修复，继续启动...");
        }

        let core = self.core.read().await;
        let game_version = self.resolve_game_version(&profile).await?;

        // Apply patches from the version JSON (e.g. Fabric/Forge mainClass override)
        for patch in &game_version.patches {
            tracing::debug!(patch_id = %patch.id, patch_ver = %patch.version, "Applying patch for launch");
        }

        let required_java = game_version.java_version.as_ref().map(|j| j.major_version)
            .unwrap_or_else(|| rmccc_version::java_version_for_mc(&profile.game_version));

        let java_install = match &profile.java_path {
            Some(jh) => {
                let home = std::path::PathBuf::from(jh);
                let bin = if home.is_file() { home } else { home.join("bin").join("java") };
                rmccc_launch::JavaInstallation { path: bin, major_version: required_java, vendor: String::new(), is_64bit: true }
            }
            None => rmccc_launch::find_java(required_java)?,
        };

        let libs_dir = profile.game_dir.join("libraries");
        let mut classpath = rmccc_version::libraries::collect_classpath(&game_version.libraries, &libs_dir);
        let client_jar = profile.game_dir.join("versions").join(&profile.game_version).join(format!("{}.jar", profile.game_version));

        if !client_jar.exists() {
            return Err(rmccc_error::RmcccError::Other(format!(
                "客户端 JAR 不存在: {}\n请先运行: rmcli resource download -i {} -v {}",
                client_jar.display(), instance_id, profile.game_version
            )));
        }
        let jar_size = std::fs::metadata(&client_jar).map(|m| m.len()).unwrap_or(0);
        if jar_size == 0 {
            return Err(rmccc_error::RmcccError::Other(format!(
                "客户端 JAR 文件为空: {}\n请重新下载: rmcli resource download -i {} -v {}",
                client_jar.display(), instance_id, profile.game_version
            )));
        }
        println!("📦 客户端 JAR: {} ({} MB)", client_jar.display(), jar_size / 1024 / 1024);
        println!("📚 类路径: {} 个条目", classpath.len() + 1);
        classpath.push(client_jar);

        let (username, uuid_str, access_token, user_type) = {
            let dir = self.data_dir().await;
            let vault = rmccc_auth::TokenVault::init(&dir.join(".key"))?;
            match vault.load_session(&dir.join("auth.json"))? {
                Some(session) => {
                    let ut = if session.provider == "microsoft" { "msa" } else { "legacy" };
                    (session.username, session.uuid, session.access_token, ut.to_string())
                }
                None => {
                    let name = "Player".to_string();
                    (name.clone(), rmccc_auth::OfflineAuthProvider::uuid_from_username(&name), "0".to_string(), "legacy".to_string())
                }
            }
        };

        let version_type = match game_version.release_type {
            rmccc_version::ReleaseType::Release => "release",
            rmccc_version::ReleaseType::Snapshot => "snapshot",
            rmccc_version::ReleaseType::OldBeta => "old_beta",
            rmccc_version::ReleaseType::OldAlpha => "old_alpha",
        };

        // ── Determine modloader type for env vars ──
        let modloader_type = profile.mod_loader.as_ref().map(|ml| ml.loader_type.as_str());

        // ── Native library decompression ──
        let natives_dir = profile.game_dir.join("versions").join(&profile.game_version).join("natives");
        println!("📦 解压 Native 库到: {}", natives_dir.display());
        let natives_count = rmccc_launch::decompress_natives(
            &game_version.libraries,
            &libs_dir,
            &natives_dir,
            false,  // don't skip GLFW
            false,  // don't skip OpenAL
        )?;
        if natives_count > 0 {
            println!("✅ Native 库: {} 个文件", natives_count);
        }

        // ── Resolve version JSON JVM arguments (e.g. -XstartOnFirstThread on macOS) ──
        let version_jvm_args = if let Some(ref args) = game_version.arguments {
            if let Some(ref jvm_args) = args.jvm {
                // Java classpath separator: ':' on Unix, ';' on Windows
                #[cfg(target_os = "windows")]
                let cp_sep = ";";
                #[cfg(not(target_os = "windows"))]
                let cp_sep = ":";
                let classpath_str = classpath.iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(cp_sep);
                let assets_dir = profile.game_dir.join("assets");
                let vars = rmccc_version::arguments::build_template_vars(
                    &username,
                    &uuid_str,
                    &access_token,
                    &user_type,
                    &profile.game_version,
                    version_type,
                    &profile.game_dir.display().to_string(),
                    &assets_dir.display().to_string(),
                    &game_version.asset_index.id,
                    &natives_dir.display().to_string(),
                    &classpath_str,
                    "RMCL",
                    env!("CARGO_PKG_VERSION"),
                    None, // width
                    None, // height
                );
                let features: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
                rmccc_version::arguments::resolve_arguments(jvm_args, &vars, &features)
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        // ── Build JVM arguments with the new CommandBuilder ──
        let jvm_args = rmccc_launch::LaunchArgBuilder::jvm_args(
            &classpath,
            profile.memory_mb,
            &profile.jvm_args,
            &version_jvm_args,
            &natives_dir,
            &profile.game_dir,
            "RMCL",
            env!("CARGO_PKG_VERSION"),
            java_install.major_version,
            java_install.is_64bit,
        );

        // ── Build game arguments ──
        let game_args = rmccc_launch::LaunchArgBuilder::game_args(
            &profile.game_version, &profile.game_dir, &username, &uuid_str, &access_token,
            &user_type, &game_version.asset_index.id, version_type, &profile.game_args,
        );

        // ── Response file ──
        let response_file = rmccc_launch::build_response_file(&jvm_args, &game_args, &game_version.main_class, &profile.game_dir)?;

        // ── Environment variables ──
        let game_env = rmccc_launch::build_game_env(
            &profile.name,
            &profile.game_dir,
            &profile.game_dir,
            &java_install.path,
            modloader_type,
        );

        println!("🚀 启动 Minecraft {} (Java {})", profile.game_version, java_install.major_version);
        println!("{}", "─".repeat(50));

        // ── Spawn with env vars ──
        let child = rmccc_launch::spawn_game(
            &java_install.path,
            &response_file,
            &profile.game_dir,
            Some(game_env),
        ).await?;
        let managed = Arc::new(rmccc_launch::ManagedProcess::new(
            instance_id.to_string(), child, 2000,
        ));
        let _monitor_handle = managed.clone().start_monitors();

        loop {
            let lines = managed.drain_lines().await;
            for line in &lines { println!("{}", line); }
            if !managed.is_running().await {
                let final_lines = managed.drain_lines().await;
                for line in &final_lines { println!("{}", line); }
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        println!("{}", "─".repeat(50));
        println!("游戏已退出。");

        drop(core);
        {
            let core = self.core.read().await;
            if let Ok(mut p) = core.profile_store.get(instance_id) {
                if let Some(ref mut p) = p { p.last_played = Some(chrono::Utc::now()); let _ = core.profile_store.save(p); }
            }
        }
        Ok(())
    }

    // ── Config ──

    pub async fn config_get(&self, key: Option<&str>) -> Result<serde_json::Value> {
        let core = self.core.read().await;
        let v = serde_json::to_value(&core.config)?;
        match key { Some(k) => Ok(v.get(k).cloned().unwrap_or(serde_json::Value::Null)), None => Ok(v) }
    }

    pub async fn config_set(&self, key: &str, value: &str) -> Result<()> {
        let mut core = self.core.write().await;
        let mut cv = serde_json::to_value(&core.config)?;
        let pv: serde_json::Value = serde_json::from_str(value).unwrap_or(serde_json::Value::String(value.to_string()));
        cv[key] = pv;
        core.config = serde_json::from_value(cv)?;
        core.config.save(&core.data_dir)?;
        Ok(())
    }
}

fn progress_cb(label: &str) -> std::sync::Arc<dyn Fn(rmccc_download::DownloadProgress) + Send + Sync> {
    let label = label.to_string();
    std::sync::Arc::new(move |p: rmccc_download::DownloadProgress| {
        let pct = if p.total_files > 0 { (p.completed_files * 100 / p.total_files) as u32 } else { 0 };
        eprint!("\r\x1b[K{}: {}/{} ({}%, {} MB)", label, p.completed_files, p.total_files, pct, p.downloaded_bytes / 1024 / 1024);
    })
}
