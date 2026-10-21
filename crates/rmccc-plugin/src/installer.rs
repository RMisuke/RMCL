//! 安装暂存区：staging → verify（安装器在提交前完成）→ commit（由 manager 执行）。
//!
//! 事务边界（ADR-003 决策 4）：
//! - 解压只发生在 `plugin-state/staging/<uuid>/`；
//! - 目标版本目录通过同卷 `rename` 原子出现；
//! - 注册表提交失败时删除新版本目录并恢复旧注册表。

use crate::capability::plugin_error;
use crate::config::load_schema;
use crate::manifest::ManifestContext;
use crate::package::{
    extract_package, extract_package_bytes, inspect_package, inspect_package_bytes,
    ArchiveInspection,
};
use rmccc_error::{PluginErrorCode, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 已解压并完成校验的包，等待原子提交。
#[derive(Debug)]
pub struct StagedPackage {
    /// `plugin-state/staging/<uuid>/`
    pub root: PathBuf,
    /// `.../unpacked/`
    pub content_dir: PathBuf,
    pub inspection: ArchiveInspection,
    pub config_schema: Option<Value>,
}

impl StagedPackage {
    /// 清理整个 staging 目录（成功/失败都要调用）。
    pub fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn stage_package_bytes(
    staging_root: &Path,
    bytes: &[u8],
    ctx: &ManifestContext,
) -> Result<StagedPackage> {
    let inspection = inspect_package_bytes(bytes, ctx)?;
    std::fs::create_dir_all(staging_root)?;
    let root = staging_root.join(uuid::Uuid::new_v4().to_string());
    let content_dir = root.join("unpacked");
    std::fs::create_dir_all(&root)?;

    let staged = (|| -> Result<StagedPackage> {
        extract_package_bytes(bytes, &content_dir)?;
        finish_staging(root.clone(), content_dir.clone(), inspection, ctx)
    })();

    if staged.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    staged
}

pub fn stage_package_path(
    staging_root: &Path,
    package: &Path,
    ctx: &ManifestContext,
) -> Result<StagedPackage> {
    let inspection = inspect_package(package, ctx)?;
    std::fs::create_dir_all(staging_root)?;
    let root = staging_root.join(uuid::Uuid::new_v4().to_string());
    let content_dir = root.join("unpacked");
    std::fs::create_dir_all(&root)?;

    let staged = (|| -> Result<StagedPackage> {
        extract_package(package, &content_dir)?;
        finish_staging(root.clone(), content_dir.clone(), inspection, ctx)
    })();

    if staged.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    staged
}

fn finish_staging(
    root: PathBuf,
    content_dir: PathBuf,
    inspection: ArchiveInspection,
    _ctx: &ManifestContext,
) -> Result<StagedPackage> {
    // 解压后重新读 Manifest：防止"检查的字节"与"解压的字节"不一致。
    let manifest_path = content_dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)?;
    let manifest = crate::manifest::PluginManifest::parse(&raw)?;
    if manifest.id != inspection.manifest.id || manifest.version != inspection.manifest.version {
        return Err(plugin_error(
            PluginErrorCode::Integrity,
            "解压后的 manifest 与包校验结果不一致".to_string(),
        ));
    }

    // 配置 schema 在提交前校验，避免装完才发现不可用。
    let schema_path = content_dir.join("config.schema.json");
    let config_schema = load_schema(&schema_path)?;

    Ok(StagedPackage {
        root,
        content_dir,
        inspection,
        config_schema,
    })
}

/// 原子提交：`rename(staging/unpacked, plugins/<id>/<version>)`。
///
/// 目标已存在时**不覆盖**（可能是用户数据/上次崩溃残留），返回状态冲突。
pub fn commit_content(staged: &mut StagedPackage, target: &Path) -> Result<()> {
    if target.exists() {
        return Err(plugin_error(
            PluginErrorCode::StateConflict,
            format!("目标版本目录已存在，拒绝覆盖：{}", target.display()),
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = std::mem::take(&mut staged.content_dir);
    std::fs::rename(&content, target)?;
    // rename 后 staging 目录只剩空壳，保持 root 供 cleanup 使用。
    staged.content_dir = content;
    Ok(())
}

/// 删除一个版本目录（尽力而为；调用方决定失败是否致命）。
pub fn remove_dir_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
