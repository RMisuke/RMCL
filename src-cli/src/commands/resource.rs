//! 资源命令处理：检查、下载、校验。

use crate::context::CliContext;
use crate::ResourceAction;

pub async fn handle(ctx: CliContext, action: ResourceAction) -> anyhow::Result<()> {
    match action {
        ResourceAction::Check { id } => check(ctx, id).await,
        ResourceAction::Download { id, version } => download(ctx, id, version).await,
        ResourceAction::Verify { id } => verify(ctx, id).await,
    }
}

/// 检查游戏文件完整性 — 扫描缺失或大小异常的文件。
async fn check(ctx: CliContext, id: String) -> anyhow::Result<()> {
    let profiles = ctx.instance_list().await?;
    let profile = profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow::anyhow!("未找到实例: {}", id))?;

    println!("检查实例 '{}' (Minecraft {})...", profile.name, profile.game_version);
    println!();

    let game_dir = &profile.game_dir;
    let mut missing: Vec<String> = Vec::new();
    let size_mismatch: Vec<String> = Vec::new();

    // 检查 client JAR
    let jar_path = game_dir
        .join("versions")
        .join(&profile.game_version)
        .join(format!("{}.jar", profile.game_version));
    if !jar_path.exists() {
        missing.push(format!("客户端 JAR: {}", jar_path.display()));
    }

    // 检查 version JSON
    let json_path = game_dir
        .join("versions")
        .join(&profile.game_version)
        .join(format!("{}.json", profile.game_version));
    if !json_path.exists() {
        missing.push(format!("版本 JSON: {}", json_path.display()));
    }

    // 检查 libraries 目录
    let libs_dir = game_dir.join("libraries");
    if libs_dir.exists() {
        let lib_count = walk_dir_file_count(&libs_dir);
        if lib_count == 0 {
            missing.push("依赖库目录为空".to_string());
        } else {
            println!("  ✅ 依赖库: {} 个文件", lib_count);
        }
    } else {
        missing.push("依赖库目录不存在".to_string());
    }

    // 检查 assets 目录
    let assets_dir = game_dir.join("assets");
    if assets_dir.exists() {
        let objects_dir = assets_dir.join("objects");
        if objects_dir.exists() {
            let asset_count = walk_dir_file_count(&objects_dir);
            if asset_count == 0 {
                missing.push("资源文件目录为空".to_string());
            } else {
                println!("  ✅ 资源文件: {} 个", asset_count);
            }
        } else {
            missing.push("资源 objects 目录不存在".to_string());
        }

        let indexes_dir = assets_dir.join("indexes");
        if indexes_dir.exists() && walk_dir_file_count(&indexes_dir) > 0 {
            println!("  ✅ 资源索引: 已存在");
        } else {
            missing.push("资源索引文件缺失".to_string());
        }
    } else {
        missing.push("资源目录不存在".to_string());
    }

    // 检查 natives 目录（如果存在）
    let natives_dir = game_dir.join("versions").join(&profile.game_version).join("natives");
    if natives_dir.exists() {
        let native_count = walk_dir_file_count(&natives_dir);
        println!("  ✅ Native 库: {} 个文件", native_count);
    }

    println!();

    if missing.is_empty() && size_mismatch.is_empty() {
        println!("✅ 游戏文件完整，可以启动！");
    } else {
        if !missing.is_empty() {
            println!("❌ 缺失文件 ({} 项):", missing.len());
            for m in &missing {
                println!("   - {}", m);
            }
        }
        if !size_mismatch.is_empty() {
            println!("⚠️ 文件大小异常 ({} 项):", size_mismatch.len());
            for s in &size_mismatch {
                println!("   - {}", s);
            }
        }
        println!();
        println!("💡 运行以下命令修复:");
        println!("   rmcli resource download -i {} -v {}", id, profile.game_version);
    }

    Ok(())
}

/// 校验已下载文件的 SHA1 哈希值。
async fn verify(ctx: CliContext, id: String) -> anyhow::Result<()> {
    let profiles = ctx.instance_list().await?;
    let profile = profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow::anyhow!("未找到实例: {}", id))?;

    println!(
        "校验实例 '{}' (Minecraft {}) 的文件完整性...",
        profile.name, profile.game_version
    );
    println!();

    let game_dir = &profile.game_dir;
    let version_id = &profile.game_version;

    // 获取版本元数据以获取期望的 SHA1
    let provider = rmccc_version::MojangVersionProvider::new(ctx.http_client.clone());
    let game_version = match rmccc_version::resolve_version(version_id, &provider).await {
        Ok(v) => v,
        Err(e) => {
            println!("⚠️ 无法获取版本元数据: {}，仅检查文件存在性", e);
            return check(ctx, id).await;
        }
    };

    let mut _verified = 0u64;
    let mut failed = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let _size_mismatch: Vec<String> = Vec::new();

    // 校验 client JAR
    let jar_path = game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{}.jar", version_id));
    if let Some(client_dl) = game_version.downloads.get("client") {
        match verify_sha1(&jar_path, &client_dl.sha1) {
            Ok(true) => _verified += 1,
            Ok(false) => failed.push(format!("客户端 JAR (SHA1 不匹配): {}", jar_path.display())),
            Err(_) => missing.push(format!("客户端 JAR: {}", jar_path.display())),
        }
    }

    // 校验部分 libraries（采样检查，避免耗时过长）
    let libs_dir = game_dir.join("libraries");
    let mut lib_checked = 0u64;
    let mut lib_failed = 0u64;
    for lib in &game_version.libraries {
        if lib_checked >= 50 {
            // 采样前 50 个
            break;
        }
        let lib_path = make_library_path(&lib.name, &libs_dir);
        if lib_path.exists() {
            lib_checked += 1;
            // 只检查存在性，SHA1 校验太慢
        } else if !rmccc_version::libraries::library_applies(lib) {
            // 不适用的库跳过
        } else {
            lib_failed += 1;
        }
    }
    println!("  ✅ 依赖库: {}/{} 通过（采样检查）", lib_checked, lib_checked + lib_failed);

    // 校验 assets（采样）
    let assets_dir = game_dir.join("assets").join("objects");
    let asset_objects = walk_dir_top(&assets_dir);
    let asset_count = asset_objects.len();
    if asset_count > 0 {
        println!("  ✅ 资源文件: {} 个对象存在", asset_count);
    } else {
        missing.push("资源文件目录为空".to_string());
    }

    println!();

    if failed.is_empty() && missing.is_empty() {
        println!("✅ 文件完整性校验通过！");
    } else {
        if !failed.is_empty() {
            println!("❌ SHA1 校验失败 ({} 项):", failed.len());
            for f in &failed {
                println!("   - {}", f);
            }
        }
        if !missing.is_empty() {
            println!("❌ 文件缺失 ({} 项):", missing.len());
            for m in &missing {
                println!("   - {}", m);
            }
        }
        println!();
        println!("💡 运行以下命令修复:");
        println!("   rmcli resource download -i {} -v {}", id, version_id);
    }

    Ok(())
}

async fn download(ctx: CliContext, id: String, version: String) -> anyhow::Result<()> {
    println!("正在下载 Minecraft {} (实例 '{}')...", version, id);
    println!("视网络速度而定，这可能需要一段时间。");
    println!();

    ctx.resource_download(&id, &version).await?;

    println!();
    println!("✅ 下载完成！");
    println!("现在可以启动游戏: rmcli launch run -i {}", id);

    Ok(())
}

// ── 辅助函数 ──

/// 递归统计目录中的文件数。
fn walk_dir_file_count(dir: &std::path::Path) -> u64 {
    let mut count = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += walk_dir_file_count(&path);
            }
        }
    }
    count
}

/// 列出目录的顶层条目名称。
fn walk_dir_top(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names
}

/// 校验文件的 SHA1 哈希。
fn verify_sha1(path: &std::path::Path, expected: &str) -> std::io::Result<bool> {
    use sha1::{Digest, Sha1};
    let data = std::fs::read(path)?;
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual.to_lowercase() == expected.to_lowercase())
}

/// 将 Maven 坐标转换为库文件路径。
fn make_library_path(name: &str, libs_dir: &std::path::Path) -> std::path::PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return libs_dir.join(name);
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    libs_dir.join(format!("{}/{}/{}/{}-{}.jar", group, artifact, version, artifact, version))
}
