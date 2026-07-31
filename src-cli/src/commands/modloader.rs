//! Modloader command handlers: list versions, install.
//!
//! Uses the new [`rmccc_modloader::DependencyManager`] for version discovery
//! and patch-based installation. Falls back to legacy [`ModLoaderProvider`]
//! for Forge/NeoForge which need java_path for installer JAR execution.

use crate::context::CliContext;
use crate::ModloaderAction;
use rmccc_modloader::{create_provider, ModLoaderType, DependencyManager};

pub async fn handle(ctx: CliContext, action: ModloaderAction) -> anyhow::Result<()> {
    match action {
        ModloaderAction::List { loader_type, game_version } => list(ctx, loader_type, game_version).await,
        ModloaderAction::Install { profile_id, loader_type, loader_version } => install(ctx, profile_id, loader_type, loader_version).await,
    }
}

async fn list(ctx: CliContext, loader_type: String, game_version: String) -> anyhow::Result<()> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| {
        anyhow::anyhow!("未知的模组加载器类型: {}. 支持: {}", loader_type, ModLoaderType::all().iter().map(|t| t.library_id()).collect::<Vec<_>>().join(", "))
    })?;

    println!("获取 {} 的 {} 版本...", game_version, lt.display_name());

    let dm = DependencyManager::new(ctx.http_client.clone());
    let versions = dm.list_versions(&lt, &game_version).await?;

    if versions.is_empty() {
        println!("未找到 {} 的 {} 可用版本", game_version, lt.display_name());
        return Ok(());
    }

    println!();
    println!("{:<20} {}", "版本", "稳定");
    println!("{}", "-".repeat(30));
    for v in &versions {
        println!("{:<20} {}", v.version, if v.stable { "✅" } else { "⚠️ 快照" });
    }
    println!();
    println!("共 {} 个版本", versions.len());
    println!();
    println!("安装: rmcli modloader install -p <实例ID> -l {} -v <版本号>", lt.library_id());

    Ok(())
}

async fn install(ctx: CliContext, profile_id: String, loader_type: String, loader_version: String) -> anyhow::Result<()> {
    let lt = ModLoaderType::from_str(&loader_type).ok_or_else(|| {
        anyhow::anyhow!("未知的模组加载器类型: {}", loader_type)
    })?;

    let (profile, game_dir, java_path) = {
        let profiles = ctx.instance_list().await?;
        let profile = profiles.iter().find(|p| p.id == profile_id)
            .ok_or_else(|| anyhow::anyhow!("未找到实例: {}", profile_id))?.clone();
        let game_dir = profile.game_dir.clone();
        let required_java = rmccc_version::java_version_for_mc(&profile.game_version);
        let java_install = rmccc_launch::find_java(required_java)?;
        (profile, game_dir, java_install.path)
    };

    println!("正在为 '{}' 安装 {} {}...", profile.name, lt.display_name(), loader_version);

    // Use legacy provider for Forge/NeoForge (need java_path for installer JAR)
    // Use new DependencyManager for Fabric/Quilt/LiteLoader/OptiFine
    let version_id = match &lt {
        ModLoaderType::Forge | ModLoaderType::NeoForge => {
            let provider = create_provider(&lt, ctx.http_client.clone());
            provider.install(&profile.game_version, &loader_version, &game_dir, &java_path).await?
        }
        _ => {
            let dm = DependencyManager::new(ctx.http_client.clone());
            // Load game version metadata
            let provider = rmccc_version::MojangVersionProvider::new(ctx.http_client.clone());
            let base_version = rmccc_version::resolve_version(&profile.game_version, &provider).await?;
            let updated = dm.install(&lt, &profile.game_version, &base_version, &loader_version, &game_dir, &java_path).await?;
            // Save the updated version JSON
            let version_dir = game_dir.join("versions").join(&updated.id);
            std::fs::create_dir_all(&version_dir)?;
            let json_path = version_dir.join(format!("{}.json", updated.id));
            std::fs::write(&json_path, serde_json::to_string_pretty(&updated)?)?;
            updated.id
        }
    };

    println!("✅ {} {} 安装成功！", lt.display_name(), loader_version);
    println!("   新版本 ID: {}", version_id);
    println!();
    println!("下一步:");
    println!("  rmcli resource download -i {} -v {}", profile_id, version_id);
    println!("  rmcli launch run -i {}", profile_id);

    // Update profile with modloader info
    {
        let core = ctx.core.read().await;
        if let Ok(mut p) = core.profile_store.get(&profile_id) {
            if let Some(ref mut p) = p {
                p.mod_loader = Some(rmccc_profile::ModLoaderConfig {
                    loader_type: lt.library_id().to_string(),
                    loader_version: loader_version.clone(),
                });
                p.game_version = version_id;
                let _ = core.profile_store.save(p);
            }
        }
    }

    Ok(())
}
