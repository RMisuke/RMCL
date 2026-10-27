//! Default launcher — mirrors HMCL's `DefaultLauncher`.
//!
//! The single implementation of the [`Launcher`] contract. It:
//! - generates the full command line via [`generate_command_line`](Self::generate_command_line)
//!   (HMCL's 20-step pipeline)
//! - extracts native libraries (with `ExtractRules` filtering)
//! - extracts the log4j configuration
//! - spawns the game with the [`ManagedProcess`] monitor wiring
//! - writes cross-platform launch scripts
//!
//! The launcher is decoupled from the download subsystem: pre-launch integrity
//! checks and downloads are orchestrated by the caller (Tauri / CLI pipeline).

use crate::command::Command;
use crate::launcher::{LaunchError, LaunchResult, Launcher};
use crate::log4j;
use crate::managed::ManagedProcess;
use crate::natives;
use crate::options::LaunchOptions;
use crate::process::{build_response_file, spawn_game, CommandBuilder};
use crate::process_listener::{AuthInfo, ExitType, ProcessListener};
use rmccc_version::arguments::vars;
use rmccc_version::{
    build_template_vars, file_layout, libraries, resolve_arguments, GameVersion, ReleaseType,
    VersionArgument,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Build a default launcher from its parts (HMCL `new DefaultLauncher(...)`).
#[allow(clippy::too_many_arguments)]
pub fn new_default_launcher(
    instance_id: String,
    game_version: GameVersion,
    auth: AuthInfo,
    options: LaunchOptions,
    listener: Option<Arc<dyn ProcessListener>>,
) -> DefaultLauncher {
    DefaultLauncher::new(instance_id, game_version, auth, options, listener)
}

/// Concrete launcher implementation (HMCL `DefaultLauncher`).
pub struct DefaultLauncher {
    /// Version / instance id (e.g. `1.20.4` or `1.20.4-fabric-0.16.9`).
    instance_id: String,
    /// Shared game root (`.minecraft` equivalent) holding `versions/`,
    /// `libraries/` and `assets/` — always shared across instances.
    game_dir: PathBuf,
    /// The instance's **run directory**: what `--gameDir` and the process working
    /// directory point at, and where per-instance `mods/`, `saves/` and
    /// `options.txt` live. Differs from `game_dir` only when the instance is
    /// isolated (HMCL `getRunDirectory`).
    run_dir: PathBuf,
    /// The resolved launch manifest.
    game_version: GameVersion,
    /// Auth credentials.
    auth: AuthInfo,
    /// Launch options.
    options: LaunchOptions,
    /// Process lifecycle listener.
    listener: Option<Arc<dyn ProcessListener>>,
}

impl DefaultLauncher {
    /// Create a new launcher.
    pub fn new(
        instance_id: String,
        game_version: GameVersion,
        auth: AuthInfo,
        options: LaunchOptions,
        listener: Option<Arc<dyn ProcessListener>>,
    ) -> Self {
        // Both directories live on `LaunchOptions`; reading them from there keeps
        // a single source of truth for "shared root" vs "run directory". An unset
        // run directory means "not isolated", i.e. the shared root — never an
        // empty path, which would make the process working directory the CWD.
        let game_dir = options.game_dir.clone();
        let run_dir = if options.run_dir.as_os_str().is_empty() {
            game_dir.clone()
        } else {
            options.run_dir.clone()
        };
        Self {
            instance_id,
            game_dir,
            run_dir,
            game_version,
            auth,
            options,
            listener,
        }
    }

    /// The instance's run directory (`--gameDir`, cwd, per-instance mods).
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// The actual Minecraft version string (the root of the inheritance chain).
    ///
    /// For inheriting instances `resolved.launch.jar` points at the base
    /// version id (e.g. `1.20.4` for `1.20.4-fabric-...`); vanilla instances
    /// fall back to the instance id.
    pub fn mc_version(&self) -> String {
        self.game_version
            .jar
            .as_deref()
            .unwrap_or(&self.game_version.id)
            .to_string()
    }

    /// `{game_dir}/versions/{id}` (HMCL `getInstanceRoot`).
    pub fn instance_root(&self) -> PathBuf {
        file_layout::instance_root(&self.game_dir, &self.instance_id)
    }

    /// Path of the `.abnormal` crash marker (HMCL `markInstanceLaunchedAbnormally`).
    pub fn abnormal_marker_path(&self) -> PathBuf {
        self.instance_root().join(".abnormal")
    }

    /// Mark the instance as launched by writing the `.abnormal` marker.
    pub fn mark_launched_abnormally(&self) {
        let path = self.abnormal_marker_path();
        if let Err(e) = std::fs::write(&path, b"") {
            tracing::warn!(
                path = %path.display(),
                error = ?e,
                "Failed to write abnormal marker"
            );
        }
    }

    /// Clear the `.abnormal` marker after a normal exit (HMCL clears on launch).
    pub fn clear_launched_abnormally(&self) {
        let path = self.abnormal_marker_path();
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %path.display(),
                    error = ?e,
                    "Failed to clear abnormal marker"
                );
            }
        }
    }

    /// The natives directory (HMCL `natives-{platform}`, legacy fallback).
    pub fn natives_dir(&self) -> PathBuf {
        file_layout::resolve_native_dir(
            &self.game_dir,
            &self.instance_id,
            &self.options.platform.descriptor,
        )
    }

    /// The client JAR path (resolved via `jar` override, HMCL `getInstanceJar`).
    pub fn client_jar_path(&self) -> PathBuf {
        let jar_id = self
            .game_version
            .jar
            .as_deref()
            .unwrap_or(&self.instance_id);
        file_layout::instance_jar(&self.game_dir, jar_id)
    }

    /// Whether this version uses log4j (1.7+).
    pub fn is_using_log4j(&self) -> bool {
        log4j::is_using_log4j(&self.mc_version())
    }

    /// Extract the log4j configuration into the instance root.
    pub fn extract_log4j_configuration_file(&self) -> LaunchResult<()> {
        log4j::extract_log4j_configuration_file(
            &self.instance_root(),
            &self.mc_version(),
            self.options.enable_debug_log_output,
        )?;
        Ok(())
    }

    /// Decompress native libraries into `natives_dir` (HMCL `decompressNatives`).
    pub fn decompress_natives(&self, natives_dir: &Path) -> LaunchResult<u32> {
        let libs_dir = file_layout::libraries_dir(&self.game_dir);
        let count = natives::decompress_natives(
            &self.game_version.libraries,
            &libs_dir,
            natives_dir,
            self.options.use_native_glfw,
            self.options.use_native_openal,
            &self.options.platform.descriptor,
        )?;
        Ok(count)
    }

    /// Feature flags for rule-gated arguments (HMCL `getFeatures`).
    pub fn get_features(&self) -> HashMap<String, bool> {
        let custom_resolution =
            self.options.width.is_some_and(|w| w > 0) && self.options.height.is_some_and(|h| h > 0);
        let mut features = HashMap::new();
        features.insert("has_custom_resolution".to_string(), custom_resolution);
        features
    }

    /// Template variables for `${...}` substitution (HMCL `getConfigurations`).
    pub fn get_configurations(
        &self,
        classpath: &str,
        natives_var: &str,
    ) -> HashMap<String, String> {
        let version_name = self
            .options
            .version_name
            .clone()
            .unwrap_or_else(|| self.game_version.id.clone());
        let version_type = self.options.version_type.clone().unwrap_or_else(|| {
            match self.game_version.release_type {
                ReleaseType::Release => "release",
                ReleaseType::Snapshot => "snapshot",
                ReleaseType::OldBeta => "old_beta",
                ReleaseType::OldAlpha => "old_alpha",
            }
            .to_string()
        });
        let assets_dir = file_layout::assets_dir(&self.game_dir)
            .display()
            .to_string();

        let mut vars_map = build_template_vars(
            &self.options.platform.descriptor,
            &self.auth.username,
            &self.auth.uuid,
            &self.auth.access_token,
            &self.auth.user_type,
            &version_name,
            &version_type,
            &self.run_dir.display().to_string(),
            &assets_dir,
            &self.game_version.asset_index.id,
            natives_var,
            classpath,
            "SNGL",
            env!("CARGO_PKG_VERSION"),
            self.options.width,
            self.options.height,
        );
        vars_map.insert(vars::PROFILE_NAME.to_string(), {
            self.options
                .profile_name
                .clone()
                .unwrap_or_else(|| "Minecraft".to_string())
        });
        vars_map.insert(
            vars::USER_PROPERTIES.to_string(),
            self.auth.user_properties.clone(),
        );

        // `${library_directory}` 必须指向共享 libraries 根（HMCL
        // DefaultLauncher "library_directory" → repository.getLibrariesDirectory），
        // 而不是 build_template_vars 推导的 `{run_dir}/libraries`：版本隔离时
        // run_dir 是实例根，Forge/NeoForge 的 `-p ${library_directory}/...`
        // 模块路径会全部落空 → JVM "Unknown module" 警告 → `--add-opens`
        // 被静默忽略 → BootstrapLauncher 反射 IMPL_LOOKUP 崩溃
        // （InaccessibleObjectException，2026-10-25 Forge 47.4.5 实测）。
        vars_map.insert(
            vars::LIBRARY_DIRECTORY.to_string(),
            file_layout::libraries_dir(&self.game_dir)
                .display()
                .to_string(),
        );

        // HMCL extras not covered by build_template_vars.
        vars_map.insert("game_assets".to_string(), assets_dir.clone());
        vars_map.insert("assets_root".to_string(), assets_dir);
        let primary_jar = self.client_jar_path();
        vars_map.insert(
            vars::PRIMARY_JAR.to_string(),
            primary_jar.display().to_string(),
        );
        vars_map.insert(
            vars::PRIMARY_JAR_NAME.to_string(),
            primary_jar
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        vars_map.insert(
            "libraries_directory".to_string(),
            file_layout::libraries_dir(&self.game_dir)
                .display()
                .to_string(),
        );
        let sep = if self.options.platform.descriptor.is_windows() {
            "\\"
        } else {
            "/"
        };
        vars_map.insert("file_separator".to_string(), sep.to_string());
        vars_map
    }

    /// Environment variables injected into the game process (HMCL `getEnvVars`).
    pub fn get_env_vars(&self) -> HashMap<String, String> {
        let version_name = self
            .options
            .version_name
            .clone()
            .unwrap_or_else(|| self.game_version.id.clone());

        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("INST_NAME".to_string(), version_name.clone());
        env.insert("INST_ID".to_string(), version_name);
        env.insert(
            "INST_DIR".to_string(),
            self.instance_root().display().to_string(),
        );
        env.insert(
            "INST_MC_DIR".to_string(),
            self.run_dir.display().to_string(),
        );
        env.insert(
            "INST_JAVA".to_string(),
            self.options.java.path.display().to_string(),
        );

        for marker in detect_modloader_markers(&self.game_version.libraries) {
            env.insert(format!("INST_{marker}"), "1".to_string());
        }

        // User-defined environment variables take precedence.
        for (k, v) in &self.options.env_vars {
            env.insert(k.clone(), v.clone());
        }
        env
    }

    /// Build the classpath (libraries + client JAR) (HMCL `repository.getClasspath`).
    pub fn get_classpath(&self) -> LaunchResult<Vec<PathBuf>> {
        let libs_dir = file_layout::libraries_dir(&self.game_dir);
        let mut classpath = libraries::collect_classpath(
            &self.game_version.libraries,
            &libs_dir,
            &self.options.platform.descriptor,
        );

        let client_jar = self.client_jar_path();
        if !client_jar.is_file() {
            return Err(LaunchError::MissingClientJar(
                client_jar.display().to_string(),
            ));
        }
        classpath.push(client_jar);
        Ok(classpath)
    }

    /// The full command line (HMCL `generateCommandLine`, 20-step pipeline).
    pub fn generate_command_line(&self) -> LaunchResult<Command> {
        let mut cmd = CommandBuilder::new();
        let java = &self.options.java;
        let jv = java.major_version;
        let is_64bit = java.is_64bit;

        // 1. Process-level prefix: priority (Unix `nice -n N`, empty elsewhere)
        //    followed by the wrapper command. Both wrap the JVM at the OS
        //    process layer and must NOT become JVM arguments — with the
        //    `@argfile` spawn model, `jvm_args` land in the response file where
        //    a leading non-`-` token is parsed as the main class
        //    ("could not find or load main class nice").
        let mut prefix = self
            .options
            .platform
            .process
            .priority_command_prefix(self.options.process_priority);
        if let Some(wrapper) = &self.options.wrapper {
            prefix.extend(wrapper.split_whitespace().map(str::to_string));
        }

        // 2. Java executable.
        //
        // Deliberately NOT added to the argument list: `Command::jvm_args` holds
        // only JVM *arguments*. The executable is supplied by the two consumers
        // — `spawn_game()` passes it as the program, and `make_launch_script()`
        // prepends it. Including it here put the java path into the response
        // file, where the JVM read it as the main class and died with
        // "could not find or load main class /Library/.../bin/java".

        // 4. Override JVM args (highest priority).
        cmd.add_all(&self.options.override_jvm_args);

        // 5. Memory.
        if self.options.max_memory > 0 {
            cmd.add_default("-Xmx", &format!("{}m", self.options.max_memory));
        }
        if self.options.min_memory > 0
            && (self.options.max_memory == 0 || self.options.min_memory <= self.options.max_memory)
        {
            cmd.add_default("-Xms", &format!("{}m", self.options.min_memory));
        }
        if self.options.metaspace > 0 {
            if jv < 8 {
                cmd.add_default("-XX:PermSize=", &format!("{}m", self.options.metaspace));
            } else {
                cmd.add_default(
                    "-XX:MetaspaceSize=",
                    &format!("{}m", self.options.metaspace),
                );
            }
        }

        // 6. User JVM args.
        cmd.add_all_default(&self.options.jvm_args);

        // 7. Encoding.
        let encoding = self.options.platform.descriptor.default_encoding();
        cmd.add_default("-Dfile.encoding=", encoding);
        if jv < 19 {
            cmd.add_default("-Dsun.stdout.encoding=", encoding);
            cmd.add_default("-Dsun.stderr.encoding=", encoding);
        } else {
            cmd.add_default("-Dstdout.encoding=", encoding);
            cmd.add_default("-Dstderr.encoding=", encoding);
        }

        // 8. Log4j RCE fixes.
        cmd.add_default("-Djava.rmi.server.useCodebaseOnly=", "true");
        cmd.add_default("-Dcom.sun.jndi.rmi.object.trustURLCodebase=", "false");
        cmd.add_default("-Dcom.sun.jndi.cosnaming.object.trustURLCodebase=", "false");
        let format_msg_no_lookups = cmd.add_default("-Dlog4j2.formatMsgNoLookups=", "true");
        if self.is_using_log4j()
            && (self.options.enable_debug_log_output
                || format_msg_no_lookups.as_deref() != Some("-Dlog4j2.formatMsgNoLookups=false"))
        {
            let log4j_file = log4j::log4j_config_file(&self.instance_root());
            cmd.add_default(
                "-Dlog4j.configurationFile=",
                &log4j_file.display().to_string(),
            );
        }

        // 9-11. Generated JVM arguments.
        if !self.options.no_generated_jvm_args {
            // Launcher brand/version (HMCL HMCLGameLauncher.appendJvmArgs).
            cmd.add_default("-Dminecraft.launcher.brand=", "SNGL");
            cmd.add_default("-Dminecraft.launcher.version=", env!("CARGO_PKG_VERSION"));

            // -Dminecraft.client.jar with the real path (L12).
            let client_jar = self.client_jar_path();
            cmd.add_default("-Dminecraft.client.jar=", &client_jar.display().to_string());

            // 平台专属 JVM 参数（macOS `-Xdock:name`）：数据由适配层给，
            // 领域代码不写 `cfg(target_os)`；实例名在此替换。
            for (key, value) in self.options.platform.descriptor.extra_jvm_args() {
                let value = value.replace("{instance}", &self.instance_id);
                cmd.add_default(key, &value);
            }

            // 非 Windows 上把 `user.home` 指到 game_dir 的父目录（HMCL 对齐）。
            if !self.options.platform.descriptor.is_windows() {
                if let Some(parent) = self.game_dir.parent() {
                    cmd.add_default("-Duser.home=", &parent.display().to_string());
                }
            }

            // Proxy: use system proxies unless the user supplied proxy args.
            let add_proxy = cmd.none_match(|a| {
                a.starts_with("-Djava.net.useSystemProxies=")
                    || a.starts_with("-Dhttp.proxy")
                    || a.starts_with("-Dhttps.proxy")
                    || a.starts_with("-DsocksProxy")
                    || a.starts_with("-Djava.net.socks.")
            });
            if add_proxy {
                cmd.add("-Djava.net.useSystemProxies=true");
            }

            if !self.options.no_generated_optimizing_jvm_args {
                add_optimizing_jvm_args(&mut cmd, jv, is_64bit);
            }

            // Java-version-specific compatibility.
            if jv == 16 {
                cmd.add_default("--illegal-access=", "permit");
            }
            if jv == 24 || jv == 25 {
                cmd.add_default("--sun-misc-unsafe-memory-access=", "allow");
            }

            // FML mod loader fixes.
            cmd.add_default("-Dfml.ignoreInvalidMinecraftCertificates=", "true");
            cmd.add_default("-Dfml.ignorePatchDiscrepancies=", "true");
        }

        // 12. Classpath.
        let classpath = self.get_classpath()?;
        let separator = self.options.platform.descriptor.classpath_separator();
        let classpath_str = classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(separator);
        cmd.add("-cp");
        cmd.add(&classpath_str);

        // 13. Natives directory (subdir from `-Djava.library.path=${natives_directory}/...`).
        let natives_dir = self.natives_dir();
        let mut java_native_folder = natives_dir.clone();
        if let Some(jvm_args) = self
            .game_version
            .arguments
            .as_ref()
            .and_then(|a| a.jvm.as_deref())
        {
            for arg in jvm_args {
                if let VersionArgument::Simple(s) = arg {
                    if let Some(sub) = s.strip_prefix("-Djava.library.path=${natives_directory}/") {
                        let candidate = natives_dir.join(sub);
                        // Conservatively only accept subdirs nested under natives_dir.
                        if candidate.starts_with(&natives_dir) {
                            java_native_folder = candidate;
                            break;
                        }
                    }
                }
            }
        }

        // Non-ASCII natives path workaround (HMCL #1141): on Linux/macOS and
        // MC < 1.19, point `${natives_directory}` at a temp symlink as well.
        let mut temp_native_folder: Option<PathBuf> = None;
        let native_str = java_native_folder.display().to_string();
        let natives_var = if self
            .options
            .platform
            .descriptor
            .needs_non_ascii_natives_workaround()
            && !native_str.is_ascii()
            && log4j::mc_version_before(&self.mc_version(), 19)
        {
            let temp = std::env::temp_dir().join(format!("rmccc-natives-{}", uuid::Uuid::new_v4()));
            temp_native_folder = Some(temp.clone());
            format!("{}:{}", temp.display(), native_str)
        } else {
            native_str
        };

        let configurations = self.get_configurations(&classpath_str, &natives_var);

        // 14. Version JSON JVM arguments (with templates + features).
        let features = self.get_features();
        if let Some(jvm_args) = self
            .game_version
            .arguments
            .as_ref()
            .and_then(|a| a.jvm.as_deref())
        {
            let mut resolved = resolve_arguments(
                &self.options.platform.descriptor,
                jvm_args,
                &configurations,
                &features,
            );

            // HMCL `MaintainTask.maintainGameWithCpwBoostrapLauncher`
            // （bootstraplauncher ≥ 0.1.17 分支）：BootstrapLauncher 按 classpath
            // 条目的**文件名**匹配 `-DignoreList`，把匹配的 jar 排除在模块层之外。
            // Forge 预置的 `${version_name}.jar` 只匹配"启动器把主 jar 按版本 id
            // 命名"的布局，而我们与 HMCL 一样把原版主 jar 放在
            // `versions/<jar id>/<jar id>.jar`（如 `1.20.1.jar`）——文件名不在
            // ignoreList 里 → 被收进模块层，文件名 `1.20.1` 不是合法模块名，
            // 转成自动模块 `_1._20._1`，与 srg jar 的 `minecraft` 模块导出同包
            // → `ResolutionException: Modules minecraft and _1._20._1 export
            // package …`（2026-10-25 Forge 47.4.5 实测）。HMCL 的修法：把主 jar
            // 文件名追加进 ignoreList。
            if self.game_version.main_class.contains("BootstrapLauncher") {
                if let Some(primary_jar_name) = configurations.get(vars::PRIMARY_JAR_NAME) {
                    if !primary_jar_name.is_empty() {
                        for arg in resolved.iter_mut() {
                            if let Some(rest) = arg.strip_prefix("-DignoreList=") {
                                if !rest.split(',').any(|e| e == primary_jar_name.as_str()) {
                                    *arg = format!("-DignoreList={rest},{primary_jar_name}");
                                }
                            }
                        }
                    }
                }
            }

            cmd.add_all_default(&resolved);
        }

        // 15. Auth launch JVM args + java agents.
        for agent in &self.options.java_agents {
            cmd.add(&format!("-javaagent:{agent}"));
        }

        // 16. Main class.
        let main_class = if self.game_version.main_class.is_empty() {
            return Err(LaunchError::MissingMainClass(self.instance_id.clone()));
        } else {
            self.game_version.main_class.clone()
        };

        // 17-19. Game arguments.
        //
        // 三种来源互斥（A9 修复：此前 legacy minecraftArguments 已收录后又
        // 追加一整套 default_game_arguments，--username/--gameDir 等全量翻倍，
        // OptiFine tweaker 等按位置解析的参数会错乱）：
        // - `arguments.game`（1.13+ 新格式）；
        // - 否则 `minecraftArguments`（≤1.12 legacy 字符串，自带完整参数集）；
        // - 两者皆无时才用启动器内置默认集（HMCL getDefaultGameArguments）。
        let mut game_args = Vec::new();
        if let Some(game) = self
            .game_version
            .arguments
            .as_ref()
            .and_then(|a| a.game.as_deref())
        {
            let resolved = resolve_arguments(
                &self.options.platform.descriptor,
                game,
                &configurations,
                &features,
            );
            game_args.extend(resolved);
        } else if let Some(mc_args) = &self.game_version.minecraft_arguments {
            for token in tokenize(mc_args) {
                game_args.push(replace_templates_owned(&token, &configurations));
            }
        } else {
            game_args.extend(default_game_arguments(
                &self.auth,
                &self.game_version,
                &self.run_dir,
                &configurations,
            ));
        }
        if self.options.fullscreen {
            game_args.push("--fullscreen".to_string());
        }
        if let Some(quick_play) = &self.options.quick_play {
            match quick_play {
                crate::QuickPlay::Singleplayer(world) => {
                    game_args.push("--quickPlaySingleplayer".to_string());
                    game_args.push(world.clone());
                }
                crate::QuickPlay::Multiplayer(address) => {
                    game_args.push("--quickPlayMultiplayer".to_string());
                    game_args.push(address.clone());
                }
                crate::QuickPlay::Realms(realm) => {
                    game_args.push("--quickPlayRealms".to_string());
                    game_args.push(realm.clone());
                }
            }
        }
        for arg in &self.options.game_args {
            game_args.push(arg.clone());
        }

        // 20. Forbidden arguments (HMCL getForbiddens: -Xincgc on Java 9+).
        if jv >= 9 {
            cmd.remove_if(|a| a == "-Xincgc");
        }

        Ok(Command {
            prefix,
            jvm_args: cmd.build(),
            game_args,
            main_class,
            java_native_folder,
            temp_native_folder,
        })
    }

    /// Launch the game (HMCL `launch()`), monitors already wired.
    pub async fn launch(&self) -> LaunchResult<Arc<ManagedProcess>> {
        let command = self.generate_command_line()?;

        // Write the `.abnormal` crash marker (HMCL markInstanceLaunchedAbnormally).
        self.mark_launched_abnormally();

        // Pre-launch: create the temp natives symlink (HMCL).
        if let Some(temp) = &command.temp_native_folder {
            // 幂等重建：旧链接可能来自上次崩溃的运行。
            let _ = self.options.platform.fs.remove_symlink(temp);
            if let Err(e) = self
                .options
                .platform
                .fs
                .symlink(&command.java_native_folder, temp)
            {
                tracing::warn!(
                    temp = %temp.display(),
                    error = ?e,
                    "Failed to create temp native symlink"
                );
            }
        }

        // Decompress natives.
        if !self.options.use_custom_natives {
            self.decompress_natives(&command.java_native_folder)?;
        }

        // Extract log4j config.
        if self.is_using_log4j() {
            self.extract_log4j_configuration_file()?;
        }

        // Pre-launch command.
        if let Some(pre) = &self.options.pre_launch_command {
            run_external_command(pre, &self.get_env_vars(), &self.run_dir)
                .await
                .map_err(|e| LaunchError::Other(format!("Pre-launch command failed: {e}")))?;
        }

        // The run directory may not exist yet — an isolated instance gets its own
        // (e.g. `versions/{id}/`) and nothing has created it so far. It is also
        // the process working directory, so spawning without it fails.
        std::fs::create_dir_all(&self.run_dir)
            .map_err(|e| LaunchError::Other(format!("Cannot create run directory: {e}")))?;

        // Response file keeps --accessToken out of `ps aux` (P1-03).
        // Drop leftovers from a previous crashed run first (only stale ones, so
        // a concurrently launching instance cannot lose its own file).
        sweep_stale_response_files(&self.run_dir);

        let response_file = build_response_file(
            &command.jvm_args,
            &command.game_args,
            &command.main_class,
            &self.run_dir,
            &self.options.platform,
        )
        .map_err(LaunchError::from)?;

        let env = self.get_env_vars();

        let child = spawn_game(
            &self.options.java.path,
            &command.prefix,
            &response_file,
            &self.run_dir,
            Some(env),
            &self.options.platform,
        )
        .await
        .map_err(LaunchError::from)?;

        // Do NOT delete the response file here: the JVM reads the `@argfile`
        // while parsing its command line, which happens *after* `spawn()`
        // returns. Removing it immediately raced with that read and made the
        // game die with `could not open .../.launch-args-<uuid>` (exit code 1)
        // — i.e. every launch failed. Hand the file to a detached task that
        // removes it once the JVM has certainly consumed it, which still keeps
        // the access token off disk for the rest of the session without adding
        // latency to the launch path.
        schedule_response_file_cleanup(response_file.clone());

        // Clean up the temp natives symlink.
        if let Some(temp) = &command.temp_native_folder {
            let _ = self.options.platform.fs.remove_symlink(temp);
        }

        let managed = Arc::new(ManagedProcess::with_platform(
            self.instance_id.clone(),
            child,
            2000,
            (*self.options.platform).clone(),
        ));
        let listener = self.wrap_listener();
        if let Some(l) = listener {
            managed.add_listener(l);
        }
        managed.clone().start_monitors();
        Ok(managed)
    }

    /// The effective listener, wrapping the user one with post-exit handling.
    fn wrap_listener(&self) -> Option<Arc<dyn ProcessListener>> {
        let instance_root = self.instance_root();
        match (&self.listener, &self.options.post_exit_command) {
            (Some(l), Some(cmd)) => Some(Arc::new(PostExitListener {
                inner: Some(l.clone()),
                command: cmd.clone(),
                env: self.get_env_vars(),
                dir: self.run_dir.clone(),
                instance_root: instance_root.clone(),
            })),
            (Some(l), None) => Some(Arc::new(PostExitListener {
                inner: Some(l.clone()),
                command: String::new(),
                env: self.get_env_vars(),
                dir: self.run_dir.clone(),
                instance_root,
            })),
            (None, Some(cmd)) => Some(Arc::new(PostExitListener {
                inner: None,
                command: cmd.clone(),
                env: self.get_env_vars(),
                dir: self.run_dir.clone(),
                instance_root,
            })),
            (None, None) => None,
        }
    }
}

impl Launcher for DefaultLauncher {
    fn generate_command_line(&self) -> LaunchResult<Command> {
        DefaultLauncher::generate_command_line(self)
    }

    fn make_launch_script(&self, path: &Path) -> LaunchResult<()> {
        make_launch_script_impl(self, path)
    }
}

// ── Response file lifecycle ──

/// Prefix of the temporary `@argfile` handed to the JVM.
const RESPONSE_FILE_PREFIX: &str = ".launch-args-";

/// How long to wait before removing the response file.
///
/// The JVM reads its `@argfile` during command-line parsing, before the VM is
/// initialised — a few tens of milliseconds in practice. If the file cannot be
/// read the JVM exits immediately with code 1, so by this point it has
/// certainly been consumed.
const RESPONSE_FILE_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Removes the response file once the JVM has consumed it.
///
/// Runs detached so the launch path is not delayed. See
/// [`RESPONSE_FILE_GRACE`] for why the file must outlive `spawn()`.
fn schedule_response_file_cleanup(path: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(RESPONSE_FILE_GRACE).await;
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::debug!(path = %path.display(), "Response file removed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = ?e,
                "Failed to remove launch response file"
            ),
        }
    });
}

/// Deletes response files left behind by a crash or a forced quit.
///
/// Only files older than [`RESPONSE_FILE_GRACE`] are touched, so an instance
/// launching concurrently in the same game directory keeps its own file.
fn sweep_stale_response_files(game_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(RESPONSE_FILE_PREFIX) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > RESPONSE_FILE_GRACE);
        if stale {
            let path = entry.path();
            if std::fs::remove_file(&path).is_ok() {
                tracing::warn!(path = %path.display(), "Removed stale launch response file");
            }
        }
    }
}

// ── Launch script generation (HMCL `makeLaunchScript`) ──

fn make_launch_script_impl(launcher: &DefaultLauncher, path: &Path) -> LaunchResult<()> {
    let command = launcher.generate_command_line()?;
    let env_vars = launcher.get_env_vars();
    let is_windows = launcher.options.platform.descriptor.is_windows();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let use_powershell = ext == "ps1";

    // Extension validation (HMCL throws IllegalArgumentException).
    if !use_powershell {
        if is_windows && ext != "bat" {
            return Err(LaunchError::Other(format!(
                "The extension of {} is not 'bat' or 'ps1' in Windows",
                path.display()
            )));
        }
        if !is_windows && !matches!(ext.as_str(), "sh" | "command" | "bash") {
            return Err(LaunchError::Other(format!(
                "The extension of {} is not 'sh', 'bash', 'ps1' or 'command'",
                path.display()
            )));
        }
    }

    // Extract log4j + natives first, matching launch() side effects.
    if launcher.is_using_log4j() {
        launcher.extract_log4j_configuration_file()?;
    }
    if !launcher.options.use_custom_natives {
        launcher.decompress_natives(&command.java_native_folder)?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // The script must start with the process-level prefix (priority / wrapper)
    // followed by the Java executable — neither is carried by
    // `Command::jvm_args` (see `generate_command_line`). The order matters:
    // `nice -n 5 wrapper… java …`, i.e. the JVM arguments must not precede the
    // java executable.
    let full_args: Vec<String> = command
        .prefix
        .iter()
        .cloned()
        .chain(std::iter::once(
            launcher.options.java.path.display().to_string(),
        ))
        .chain(command.jvm_args.iter().cloned())
        .chain(std::iter::once(command.main_class.clone()))
        .chain(command.game_args.iter().cloned())
        .collect();

    // Windows command-line length limit (HMCL CommandTooLongException).
    if is_windows && !use_powershell {
        let joined = full_args.join(" ");
        if joined.len() > 32_767 {
            return Err(LaunchError::CommandTooLong(joined.len()));
        }
    }

    let run_dir = launcher.game_dir.display().to_string();
    let content = if use_powershell {
        powershell_script(&env_vars, &full_args, &run_dir, launcher)
    } else if is_windows {
        batch_script(&env_vars, &full_args, &run_dir, launcher)
    } else {
        shell_script(
            &env_vars,
            &full_args,
            &run_dir,
            command.temp_native_folder.as_ref(),
            &command.java_native_folder,
            launcher,
        )
    };

    std::fs::write(path, content)?;

    // 可执行位经平台后端设置（Unix 0755；Windows 无此概念，后端返回 no-op/不适用）。
    launcher.options.platform.fs.make_executable(path)?;
    Ok(())
}

fn shell_script(
    env: &HashMap<String, String>,
    args: &[String],
    run_dir: &str,
    temp_native_folder: Option<&PathBuf>,
    native_folder: &Path,
    launcher: &DefaultLauncher,
) -> String {
    let mut s = String::from("#!/usr/bin/env bash\n");
    for (k, v) in env {
        s.push_str(&format!("export {}={}\n", k, shell_quote(v)));
    }
    if let Some(temp) = temp_native_folder {
        s.push_str(&format!(
            "ln -s {} {}\n",
            shell_quote(&native_folder.display().to_string()),
            shell_quote(&temp.display().to_string())
        ));
    }
    s.push_str(&format!("cd {}\n", shell_quote(run_dir)));
    if let Some(pre) = &launcher.options.pre_launch_command {
        s.push_str(&format!("{}\n", pre));
    }
    let joined: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
    s.push_str(&joined.join(" "));
    s.push('\n');
    if let Some(post) = &launcher.options.post_exit_command {
        s.push_str(&format!("{}\n", post));
    }
    if let Some(temp) = temp_native_folder {
        s.push_str(&format!(
            "rm {}\n",
            shell_quote(&temp.display().to_string())
        ));
    }
    s
}

fn batch_script(
    env: &HashMap<String, String>,
    args: &[String],
    run_dir: &str,
    launcher: &DefaultLauncher,
) -> String {
    let mut s = String::from("@echo off\n");
    for (k, v) in env {
        s.push_str(&format!("set {}={}\n", k, batch_quote(v)));
    }
    s.push_str(&format!("cd /D {}\n", batch_quote(run_dir)));
    if let Some(pre) = &launcher.options.pre_launch_command {
        s.push_str(&format!("{}\n", pre));
    }
    let joined: Vec<String> = args.iter().map(|a| batch_quote(a)).collect();
    s.push_str(&joined.join(" "));
    s.push_str("\npause\n");
    if let Some(post) = &launcher.options.post_exit_command {
        s.push_str(&format!("{}\n", post));
    }
    s
}

fn powershell_script(
    env: &HashMap<String, String>,
    args: &[String],
    run_dir: &str,
    launcher: &DefaultLauncher,
) -> String {
    let mut s = String::new();
    for (k, v) in env {
        s.push_str(&format!("$Env:{}={}\n", k, pwsh_quote(v)));
    }
    s.push_str(&format!(
        "Set-Location -LiteralPath {}\n",
        pwsh_quote(run_dir)
    ));
    if let Some(pre) = &launcher.options.pre_launch_command {
        s.push_str(&format!("& {}\n", pre));
    }
    let joined: Vec<String> = args.iter().map(|a| pwsh_quote(a)).collect();
    s.push_str(&format!("& {}\n", joined.join(" ")));
    if let Some(post) = &launcher.options.post_exit_command {
        s.push_str(&format!("& {}\n", post));
    }
    s
}

// ── Helpers ──

/// Detect modloader markers for `INST_*` env vars (HMCL `LibraryAnalyzer`).
fn detect_modloader_markers(libraries: &[rmccc_version::Library]) -> Vec<&'static str> {
    let mut markers = Vec::new();
    let push = |marker: &'static str, markers: &mut Vec<&'static str>| {
        if !markers.contains(&marker) {
            markers.push(marker);
        }
    };
    for lib in libraries {
        let name = &lib.name;
        if name.starts_with("net.fabricmc:fabric-loader") {
            push("FABRIC", &mut markers);
        } else if name.starts_with("net.legacyfabric:fabric-loader") {
            push("LEGACYFABRIC", &mut markers);
        } else if name.starts_with("org.quiltmc:quilt-loader") {
            push("QUILT", &mut markers);
        } else if name.starts_with("net.neoforged") {
            push("NEOFORGE", &mut markers);
        } else if name.starts_with("net.minecraftforge:forge")
            || name.starts_with("net.minecraftforge:fmlloader")
        {
            push("FORGE", &mut markers);
        } else if name.starts_with("com.mumfrey:liteloader") {
            push("LITELOADER", &mut markers);
        } else if name.starts_with("optifine:OptiFine") || name.starts_with("optifine.OptiFine") {
            push("OPTIFINE", &mut markers);
        }
    }
    markers
}

/// GC / JIT optimizing arguments (HMCL `appendJvmArgs` optimizing block).
fn add_optimizing_jvm_args(cmd: &mut CommandBuilder, jv: u32, is_64bit: bool) {
    if jv >= 8 {
        cmd.add_unstable_bool("UnlockExperimentalVMOptions", true);
        cmd.add_unstable_bool("UnlockDiagnosticVMOptions", true);
    }
    if jv >= 8
        && cmd
            .none_match(|a| a.starts_with("-XX:+Use") && a != "-XX:+UseG1GC" || a == "-XX:-UseG1GC")
    {
        cmd.add_unstable_bool("UseG1GC", true);
        cmd.add_unstable_value("G1MixedGCCountTarget", "5");
        cmd.add_unstable_value("G1NewSizePercent", "20");
        cmd.add_unstable_value("G1ReservePercent", "20");
        cmd.add_unstable_value("MaxGCPauseMillis", "50");
        cmd.add_unstable_value("G1HeapRegionSize", "32m");
    }
    if jv >= 8 {
        cmd.add_unstable_bool("OmitStackTraceInFastThrow", false);
    }
    if is_64bit {
        cmd.add_unstable_value("ReservedCodeCacheSize", "400M");
        cmd.add_unstable_bool("DontCompileHugeMethods", false);
    } else {
        cmd.add_default("-Xss", "1m");
    }
}

/// Default game arguments for legacy versions (HMCL `getDefaultGameArguments`).
fn default_game_arguments(
    auth: &AuthInfo,
    game_version: &GameVersion,
    game_dir: &Path,
    configurations: &HashMap<String, String>,
) -> Vec<String> {
    let assets_dir = file_layout::assets_dir(game_dir);
    let raw = vec![
        "--username".to_string(),
        configurations
            .get(vars::AUTH_PLAYER_NAME)
            .cloned()
            .unwrap_or_else(|| auth.username.clone()),
        "--version".to_string(),
        game_version.id.clone(),
        "--gameDir".to_string(),
        game_dir.display().to_string(),
        "--assetsDir".to_string(),
        assets_dir.display().to_string(),
        "--assetIndex".to_string(),
        game_version.asset_index.id.clone(),
        "--uuid".to_string(),
        configurations
            .get(vars::AUTH_UUID)
            .cloned()
            .unwrap_or_default(),
        "--accessToken".to_string(),
        configurations
            .get(vars::AUTH_ACCESS_TOKEN)
            .cloned()
            .unwrap_or_else(|| auth.access_token.clone()),
        "--userType".to_string(),
        configurations
            .get(vars::USER_TYPE)
            .cloned()
            .unwrap_or_else(|| auth.user_type.clone()),
        "--versionType".to_string(),
        configurations
            .get(vars::VERSION_TYPE)
            .cloned()
            .unwrap_or_default(),
    ];
    raw
}

/// Simple whitespace tokenizer for legacy minecraftArguments.
fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

/// Template substitution for already-tokenized legacy args.
fn replace_templates_owned(arg: &str, vars: &HashMap<String, String>) -> String {
    rmccc_version::replace_templates(arg, vars)
}

/// Run a pre/post launch command (tokenized by whitespace) with env + cwd.
async fn run_external_command(
    command: &str,
    env: &HashMap<String, String>,
    dir: &Path,
) -> std::io::Result<()> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(());
    }
    let mut child = tokio::process::Command::new(tokens[0]);
    child.args(&tokens[1..]).current_dir(dir);
    for (k, v) in env {
        child.env(k, v);
    }
    let status = child.status().await?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "command exited with {:?}",
            status.code()
        )))
    }
}

/// A listener wrapper that clears the `.abnormal` marker on a normal exit and
/// runs the post-exit command (HMCL ExitWaiter + crash marker).
struct PostExitListener {
    inner: Option<Arc<dyn ProcessListener>>,
    command: String,
    env: HashMap<String, String>,
    dir: PathBuf,
    instance_root: PathBuf,
}

impl ProcessListener for PostExitListener {
    fn set_process(&self, process: &ManagedProcess) {
        if let Some(inner) = &self.inner {
            inner.set_process(process);
        }
    }

    fn on_log(&self, line: String, is_stderr: bool) {
        if let Some(inner) = &self.inner {
            inner.on_log(line, is_stderr);
        }
    }

    fn on_exit(&self, exit_code: i32, exit_type: ExitType) {
        // A normal exit clears the crash marker (HMCL markInstanceLaunchedAbnormally).
        if exit_type == ExitType::Normal {
            let marker = self.instance_root.join(".abnormal");
            if let Err(e) = std::fs::remove_file(&marker) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %marker.display(), error = ?e, "Failed to clear abnormal marker");
                }
            }
        }

        if !self.command.is_empty() {
            let command = self.command.clone();
            let env = self.env.clone();
            let dir = self.dir.clone();
            tokio::spawn(async move {
                if let Err(e) = run_external_command(&command, &env, &dir).await {
                    tracing::warn!("Post-exit command failed: {e}");
                }
            });
        }
        if let Some(inner) = &self.inner {
            inner.on_exit(exit_code, exit_type);
        }
    }
}

// ── Shell escaping helpers ──

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn batch_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn pwsh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmccc_platform::ProcessPriority;

    fn test_launcher() -> DefaultLauncher {
        let mut gv = GameVersion {
            id: "1.20.4".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            ..Default::default()
        };
        gv.asset_index.id = "17".to_string();
        let mut options = LaunchOptions {
            max_memory: 2048,
            game_dir: PathBuf::from("/game"),
            ..Default::default()
        };
        options.java.major_version = 17;
        options.java.is_64bit = true;
        DefaultLauncher::new(
            "1.20.4".to_string(),
            gv,
            AuthInfo::new("Steve", "uuid", "token", "msa"),
            options,
            None,
        )
    }

    #[test]
    fn test_mc_version_resolution() {
        let l = test_launcher();
        assert_eq!(l.mc_version(), "1.20.4");
    }

    #[test]
    fn test_features_custom_resolution() {
        let l = test_launcher();
        let f = l.get_features();
        assert!(!f["has_custom_resolution"]);

        let mut opts = l.options.clone();
        opts.width = Some(1920);
        opts.height = Some(1080);
        let l2 = DefaultLauncher::new(
            l.instance_id.clone(),
            l.game_version.clone(),
            l.auth.clone(),
            opts,
            None,
        );
        assert!(l2.get_features()["has_custom_resolution"]);
    }

    #[test]
    fn test_env_vars() {
        let l = test_launcher();
        let env = l.get_env_vars();
        assert_eq!(env.get("INST_NAME").unwrap(), "1.20.4");
        assert_eq!(env.get("INST_MC_DIR").unwrap(), "/game");
        assert!(env.contains_key("INST_JAVA"));
    }

    #[test]
    fn test_detect_modloader_markers() {
        let libs = vec![
            rmccc_version::Library {
                name: "net.fabricmc:fabric-loader:0.16.9".into(),
                ..Default::default()
            },
            rmccc_version::Library {
                name: "net.neoforged:neoforge:20.4.0".into(),
                ..Default::default()
            },
        ];
        let markers = detect_modloader_markers(&libs);
        assert!(markers.contains(&"FABRIC"));
        assert!(markers.contains(&"NEOFORGE"));
    }

    #[test]
    fn test_escaping() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(pwsh_quote("it's"), "'it''s'");
        assert_eq!(batch_quote("a\"b"), "\"a\"\"b\"");
    }

    /// Build a launcher backed by a temp game dir with a fake client JAR.
    fn test_launcher_on_disk() -> (DefaultLauncher, PathBuf) {
        test_launcher_on_disk_opts(ProcessPriority::Normal, None)
    }

    /// Same as [`test_launcher_on_disk`] with process priority / wrapper set.
    fn test_launcher_on_disk_opts(
        process_priority: ProcessPriority,
        wrapper: Option<&str>,
    ) -> (DefaultLauncher, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rmcl-launch-{}", uuid::Uuid::new_v4()));
        let game_dir = dir.join("game");
        let jar_dir = game_dir.join("versions/1.20.4");
        std::fs::create_dir_all(&jar_dir).unwrap();
        std::fs::write(jar_dir.join("1.20.4.jar"), b"fake-jar").unwrap();
        std::fs::write(jar_dir.join("1.20.4.json"), b"{}").unwrap();

        let mut gv = GameVersion {
            id: "1.20.4".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            ..Default::default()
        };
        gv.asset_index.id = "17".to_string();

        let mut options = LaunchOptions {
            max_memory: 2048,
            game_dir: game_dir.clone(),
            process_priority,
            wrapper: wrapper.map(str::to_string),
            ..Default::default()
        };
        options.java.major_version = 17;
        options.java.is_64bit = true;

        let l = DefaultLauncher::new(
            "1.20.4".to_string(),
            gv,
            AuthInfo::new("Steve", "uuid", "token", "msa"),
            options,
            None,
        );
        (l, dir)
    }

    #[test]
    fn test_generate_command_line_pipeline() {
        let (l, dir) = test_launcher_on_disk();
        let command = l.generate_command_line().unwrap();

        // Main class is present.
        assert_eq!(command.main_class, "net.minecraft.client.main.Main");
        // Memory args.
        assert!(command.jvm_args.iter().any(|a| a.starts_with("-Xmx")));
        // Classpath contains the client JAR (L12 real path).
        assert!(command
            .jvm_args
            .iter()
            .any(|a| a.ends_with("/versions/1.20.4/1.20.4.jar")));
        // -Dminecraft.client.jar uses the real path.
        assert!(command
            .jvm_args
            .iter()
            .any(|a| { a.starts_with("-Dminecraft.client.jar=") && a.contains("1.20.4.jar") }));
        // Launcher metadata injected.
        assert!(command
            .jvm_args
            .iter()
            .any(|a| a == "-Dminecraft.launcher.brand=SNGL"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Forge 47.4.5 启动崩溃回归（2026-10-25）────────────────────────
    // 版本隔离时 run_dir 是实例根；`${library_directory}` 必须仍指向共享
    // libraries 根，否则 Forge `-p ${library_directory}/...` 模块路径落空，
    // `--add-opens` 被静默忽略 → BootstrapLauncher 反射 IMPL_LOOKUP 崩溃。
    #[test]
    fn test_library_directory_points_at_shared_libraries_root() {
        let (l, dir) = test_launcher_on_disk();

        // 版本隔离：run_dir = 实例根（隔离目录下不存在 libraries/）。
        let run_dir = dir.join("game/versions/1.20.4");
        let mut opts = l.options.clone();
        opts.run_dir = run_dir.clone();
        let mut gv = l.game_version.clone();
        gv.arguments = Some(rmccc_version::VersionArguments {
            // Forge 47.x 的模块路径形态（简化）。
            jvm: Some(vec![rmccc_version::VersionArgument::Simple(
                "-p ${library_directory}/cpw/mods/securejarhandler/2.1.10/securejarhandler-2.1.10.jar"
                    .to_string(),
            )]),
            game: None,
        });
        let l = DefaultLauncher::new(l.instance_id.clone(), gv, l.auth.clone(), opts, None);
        assert!(!run_dir.join("libraries").exists());

        // 模板变量层：指向共享 libraries 根，而非实例根下的 libraries。
        let vars = l.get_configurations("cp", "/natives");
        assert_eq!(
            vars.get("library_directory").unwrap(),
            &file_layout::libraries_dir(&dir.join("game"))
                .display()
                .to_string()
        );

        // 解析后的 JVM 参数层：-p 模块路径落在共享 libraries 根。
        let command = l.generate_command_line().unwrap();
        let expected = format!(
            "-p {}/cpw/mods/securejarhandler/2.1.10/securejarhandler-2.1.10.jar",
            file_layout::libraries_dir(&dir.join("game")).display()
        );
        assert!(
            command.jvm_args.contains(&expected),
            "module path must resolve against the shared libraries root, got {:#?}",
            command.jvm_args
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Forge 47.4.5 模块冲突回归（2026-10-25）────────────────────────
    // BootstrapLauncher 按 classpath 文件名匹配 `-DignoreList`；原版主 jar
    // `1.20.1.jar` 不在 Forge 预置 ignoreList → 进模块层成 `_1._20._1`
    // 自动模块，与 srg jar 的 `minecraft` 模块导出同包 → ResolutionException。
    // 修复对齐 HMCL：把主 jar 文件名追加进 ignoreList。
    #[test]
    fn test_bootstrap_launcher_ignore_list_appends_primary_jar() {
        let (l, dir) = test_launcher_on_disk();

        // Forge 形态：BootstrapLauncher 主类 + 预置 ignoreList（含
        // `${version_name}.jar`，经模板解析后是 options.version_name）。
        let mut gv = l.game_version.clone();
        gv.main_class = "cpw.mods.bootstraplauncher.BootstrapLauncher".to_string();
        gv.arguments = Some(rmccc_version::VersionArguments {
            jvm: Some(vec![rmccc_version::VersionArgument::Simple(
                "-DignoreList=bootstraplauncher,client-extra,${version_name}.jar".to_string(),
            )]),
            game: None,
        });
        let mut opts = l.options.clone();
        opts.version_name = Some("1.20.1-forge".to_string());
        let l = DefaultLauncher::new(l.instance_id.clone(), gv, l.auth.clone(), opts, None);

        let command = l.generate_command_line().unwrap();
        // 主 jar 文件名（1.20.4.jar，来自测试夹具的 jar id）被追加到末尾。
        let expected =
            "-DignoreList=bootstraplauncher,client-extra,1.20.1-forge.jar,1.20.4.jar".to_string();
        assert!(
            command.jvm_args.contains(&expected),
            "primary jar name must be appended to ignoreList, got {:#?}",
            command.jvm_args
        );

        // 非 BootstrapLauncher 主类（vanilla / Fabric）不动 ignoreList。
        let mut gv = l.game_version.clone();
        gv.main_class = "net.minecraft.client.main.Main".to_string();
        let l2 = DefaultLauncher::new(
            l.instance_id.clone(),
            gv,
            l.auth.clone(),
            l.options.clone(),
            None,
        );
        let command2 = l2.generate_command_line().unwrap();
        assert!(
            command2
                .jvm_args
                .iter()
                .any(|a| a == "-DignoreList=bootstraplauncher,client-extra,1.20.1-forge.jar"),
            "vanilla launch must not touch ignoreList, got {:#?}",
            command2.jvm_args
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_make_launch_script_sh() {
        let (l, dir) = test_launcher_on_disk();
        let script = l.game_dir.join("start.sh");
        l.make_launch_script(&script).unwrap();
        assert!(script.exists());
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("#!/usr/bin/env bash"));
        assert!(content.contains("net.minecraft.client.main.Main"));
        assert!(content.contains("INST_NAME"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_make_launch_script_ps1() {
        let (l, dir) = test_launcher_on_disk();
        let script = l.game_dir.join("start.ps1");
        l.make_launch_script(&script).unwrap();
        assert!(script.exists());
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("$Env:INST_NAME"));
        assert!(content.contains("net.minecraft.client.main.Main"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── P1-7 regression: priority / wrapper are process-level prefixes ──

    #[test]
    fn test_priority_and_wrapper_become_process_prefix() {
        let (l, dir) = test_launcher_on_disk_opts(ProcessPriority::Low, Some("mywrap -c"));
        let command = l.generate_command_line().unwrap();

        let expected_priority = l
            .options
            .platform
            .process
            .priority_command_prefix(ProcessPriority::Low);
        assert_eq!(
            command.prefix,
            [expected_priority, vec!["mywrap".into(), "-c".into()]].concat()
        );
        // The prefix must not leak into the JVM argument domain: with the
        // `@argfile` spawn model the JVM parses a leading non-`-` token as the
        // main class and dies with "could not find or load main class nice".
        assert!(!command.jvm_args.iter().any(|a| a == "nice"));
        assert!(!command.jvm_args.iter().any(|a| a == "mywrap"));
        assert!(!command.jvm_args.iter().any(|a| a == "-c"));
        // JVM args still start with a real option.
        assert!(command.jvm_args.first().is_some_and(|a| a.starts_with('-')));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_normal_priority_has_empty_prefix() {
        let (l, dir) = test_launcher_on_disk();
        let command = l.generate_command_line().unwrap();
        assert!(command.prefix.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_launch_script_prefix_precedes_java() {
        let (l, dir) = test_launcher_on_disk_opts(ProcessPriority::Low, Some("mywrap -c"));
        let script = l.game_dir.join("start-priority.sh");
        l.make_launch_script(&script).unwrap();
        let content = std::fs::read_to_string(&script).unwrap();
        let command_line = content
            .lines()
            .find(|l| l.contains("net.minecraft.client.main.Main"))
            .expect("script contains the command line");

        // Order must be `nice … mywrap -c java … main …` — the java executable
        // sits between the prefix and the JVM arguments.
        let java_pos = command_line
            .find(&l.options.java.path.display().to_string())
            .expect("script contains java path");
        assert!(command_line.contains("nice"));
        assert!(command_line.find("mywrap").is_some_and(|p| p < java_pos));
        let xmx_pos = command_line.find("-Xmx").expect("script contains -Xmx");
        assert!(java_pos < xmx_pos);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end spawn check: a fake `java` records the argv it received.
    /// With `Low` priority the launch path must run `nice -n 1 java @file`,
    /// so the fake java sees exactly the `@argfile` — proving the priority
    /// prefix wrapped the JVM at process level and never entered the argfile.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_launch_spawns_prefix_then_java_argfile() {
        let (l, dir) = test_launcher_on_disk_opts(ProcessPriority::Low, None);

        // Fake java: shell script recording its argv, then exiting cleanly.
        let fake_java = dir.join("fake-java.sh");
        let argv_out = dir.join("fake-java-argv.txt");
        std::fs::write(
            &fake_java,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
                argv_out.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(
            &fake_java,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let mut l = l;
        l.options.java.path = fake_java.clone();

        let managed = l.launch().await.expect("launch spawns the fake java");

        // Wait for the child to record its argv and exit (it exits immediately).
        for _ in 0..100 {
            if argv_out.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let argv = std::fs::read_to_string(&argv_out).expect("fake java recorded its argv");
        let argv: Vec<String> = argv.lines().map(str::to_string).collect();

        // Exactly one argument: the response file path (with `@`).
        assert_eq!(argv.len(), 1, "fake java argv: {argv:?}");
        assert!(argv[0].starts_with('@'), "fake java argv: {argv:?}");
        // The response file existed when java read it and holds the JVM args,
        // the main class and the game args — not the priority prefix.
        let argfile_path = argv[0].trim_start_matches('@').to_string();
        let content = std::fs::read_to_string(&argfile_path).unwrap();
        assert!(content.contains("net.minecraft.client.main.Main"));
        assert!(!content.lines().any(|l| l == "nice"));

        let _ = managed.wait_exit().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A9：三种 game args 来源互斥——legacy minecraftArguments 自带完整
    /// 参数集时，不得再追加 default_game_arguments（此前 --username 等
    /// 全量翻倍）。
    #[test]
    fn test_legacy_args_are_not_doubled() {
        let (mut l, dir) = test_launcher_on_disk();
        l.game_version.minecraft_arguments = Some(
            "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory}"
                .into(),
        );
        l.game_version.arguments = None;

        let cmd = l.generate_command_line().unwrap();
        let usernames = cmd.game_args.iter().filter(|a| *a == "--username").count();
        let gamedirs = cmd.game_args.iter().filter(|a| *a == "--gameDir").count();
        assert_eq!(usernames, 1, "game args: {:?}", cmd.game_args);
        assert_eq!(gamedirs, 1, "game args: {:?}", cmd.game_args);
        // 模板已被替换为实际值。
        assert!(
            cmd.game_args.iter().any(|a| a.contains("Steve")),
            "game args: {:?}",
            cmd.game_args
        );
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A9：1.13+ `arguments.game` 存在时优先于 legacy 字符串。
    #[test]
    fn test_new_style_args_win_over_legacy_string() {
        let (mut l, dir) = test_launcher_on_disk();
        l.game_version.minecraft_arguments = Some("--legacy-only".into());
        let args = rmccc_version::VersionArguments {
            game: Some(vec![rmccc_version::VersionArgument::Simple(
                "--new-only".into(),
            )]),
            ..Default::default()
        };
        l.game_version.arguments = Some(args);

        let cmd = l.generate_command_line().unwrap();
        assert!(cmd.game_args.contains(&"--new-only".to_string()));
        assert!(!cmd.game_args.contains(&"--legacy-only".to_string()));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A9：两者皆无时使用内置默认集（保证离线可启动）。
    #[test]
    fn test_default_args_when_no_version_args() {
        let (mut l, dir) = test_launcher_on_disk();
        l.game_version.minecraft_arguments = None;
        l.game_version.arguments = None;

        let cmd = l.generate_command_line().unwrap();
        assert!(cmd.game_args.contains(&"--username".to_string()));
        assert!(cmd.game_args.contains(&"--version".to_string()));
        assert!(cmd.game_args.contains(&"--gameDir".to_string()));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
