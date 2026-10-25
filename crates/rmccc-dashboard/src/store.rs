//! 布局持久化（ADR-005 决策 2）。
//!
//! `dashboard/layouts/<layoutId>.json`，损坏备份 + 默认恢复。

use crate::layout::{normalize, ConstraintMap, DashboardLayout, NormalizationResult};
use rmccc_error::Result;
use rmccc_platform::PlatformFs;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 布局存储：读取/归一化/写入。
pub struct LayoutStore {
    layouts_dir: PathBuf,
    fs: Arc<dyn PlatformFs>,
}

/// B2：layout_id 直接进 `{layouts_dir}/{id}.json` 拼接，入口来自 IPC——
/// 只允许内部 id 形态（`[A-Za-z0-9_-]{1,64}`），拒绝路径注入
/// （审计 2026-10-25）。
fn validate_layout_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(rmccc_error::RmcccError::Validation(format!(
            "invalid layout id: {id:?}"
        )))
    }
}

impl LayoutStore {
    pub fn new(data_dir: &Path, fs: Arc<dyn PlatformFs>) -> Self {
        Self {
            layouts_dir: data_dir.join("dashboard/layouts"),
            fs,
        }
    }

    pub fn layouts_dir(&self) -> &Path {
        &self.layouts_dir
    }

    /// 加载布局；缺失 → 创建默认；损坏 → 备份 + 默认。
    pub fn load_or_init(&self, layout_id: &str) -> Result<(DashboardLayout, NormalizationResult)> {
        self.load_or_init_with(
            layout_id,
            &crate::widget::WidgetRegistry::builtin().constraint_map(),
        )
    }

    /// 带自定义约束加载（插件卡片尺寸）。
    pub fn load_or_init_with(
        &self,
        layout_id: &str,
        constraints: &ConstraintMap,
    ) -> Result<(DashboardLayout, NormalizationResult)> {
        validate_layout_id(layout_id)?;
        std::fs::create_dir_all(&self.layouts_dir)?;
        let path = self.layouts_dir.join(format!("{layout_id}.json"));

        match std::fs::read_to_string(&path) {
            Ok(json) => match DashboardLayout::parse(&json) {
                Ok(mut layout) => {
                    let result = normalize(&mut layout, constraints);
                    Ok((result.layout.clone(), result))
                }
                Err(e) => {
                    backup_and_default(&path, &e.to_string())?;
                    Ok((
                        DashboardLayout::default_layout(),
                        NormalizationResult {
                            layout: DashboardLayout::default_layout(),
                            corrections: Vec::new(),
                        },
                    ))
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let layout = DashboardLayout::default_layout();
                write_json_atomic(self.fs.as_ref(), &path, &layout)?;
                Ok((
                    layout.clone(),
                    NormalizationResult {
                        layout,
                        corrections: Vec::new(),
                    },
                ))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 归一化并保存布局。
    pub fn save(&self, layout: &mut DashboardLayout) -> Result<NormalizationResult> {
        self.save_with(
            layout,
            &crate::widget::WidgetRegistry::builtin().constraint_map(),
        )
    }

    /// 带自定义约束保存（插件卡片尺寸）。
    pub fn save_with(
        &self,
        layout: &mut DashboardLayout,
        constraints: &ConstraintMap,
    ) -> Result<NormalizationResult> {
        let result = normalize(layout, constraints);
        validate_layout_id(&result.layout.layout_id)?;
        std::fs::create_dir_all(&self.layouts_dir)?;
        let path = self
            .layouts_dir
            .join(format!("{}.json", result.layout.layout_id));
        write_json_atomic(self.fs.as_ref(), &path, &result.layout)?;
        Ok(result)
    }

    /// 重置为默认布局。
    pub fn reset(&self, layout_id: &str) -> Result<DashboardLayout> {
        let layout = DashboardLayout::default_layout();
        let path = self.layouts_dir.join(format!("{layout_id}.json"));
        std::fs::create_dir_all(&self.layouts_dir)?;
        write_json_atomic(self.fs.as_ref(), &path, &layout)?;
        Ok(layout)
    }
}

fn backup_and_default(path: &Path, reason: &str) -> Result<()> {
    let backup = path.with_extension(format!(
        "json.bak-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S")
    ));
    let _ = std::fs::rename(path, &backup);
    tracing::warn!(path = %path.display(), backup = %backup.display(), reason, "布局文件损坏，已备份并恢复默认");
    Ok(())
}

fn write_json_atomic<T: Serialize>(fs: &dyn PlatformFs, path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("layout.json"),
        uuid::Uuid::new_v4()
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs.atomic_replace(&tmp, path)?;
    Ok(())
}

/// 便捷函数：加载默认布局。
pub fn load_layout(
    data_dir: &Path,
    fs: Arc<dyn PlatformFs>,
    layout_id: &str,
) -> Result<(DashboardLayout, NormalizationResult)> {
    LayoutStore::new(data_dir, fs).load_or_init(layout_id)
}

/// 便捷函数：归一化并保存。
pub fn save_layout(
    data_dir: &Path,
    fs: Arc<dyn PlatformFs>,
    layout: &mut DashboardLayout,
) -> Result<NormalizationResult> {
    LayoutStore::new(data_dir, fs).save(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CorrectionKind;
    use rmccc_platform::Platform;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rmccc-dashboard-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_creates_default_when_missing() {
        let dir = temp_dir("missing");
        let platform = Platform::mock().0;
        let store = LayoutStore::new(&dir, platform.fs);
        let (layout, result) = store.load_or_init("default").unwrap();
        assert_eq!(layout.layout_id, "default");
        assert!(result.corrections.is_empty());
        assert!(dir.join("dashboard/layouts/default.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_layout_backed_up_and_replaced() {
        let dir = temp_dir("corrupt");
        let platform = Platform::mock().0;
        let store = LayoutStore::new(&dir, platform.fs);
        std::fs::create_dir_all(dir.join("dashboard/layouts")).unwrap();
        std::fs::write(dir.join("dashboard/layouts/default.json"), b"not json").unwrap();
        let (layout, _) = store.load_or_init("default").unwrap();
        assert_eq!(layout.layout_id, "default");
        assert_eq!(layout.widgets.len(), 3);
        let backups: Vec<_> = std::fs::read_dir(dir.join("dashboard/layouts"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_normalizes_and_persists() {
        let dir = temp_dir("save");
        let platform = Platform::mock().0;
        let store = LayoutStore::new(&dir, platform.fs);
        let mut layout = DashboardLayout::default_layout();
        layout.widgets[0].size.w = 99;
        let result = store.save(&mut layout).unwrap();
        assert!(result
            .corrections
            .iter()
            .any(|c| c.kind == CorrectionKind::SizeClamped));
        let raw = std::fs::read_to_string(dir.join("dashboard/layouts/default.json")).unwrap();
        let reloaded = DashboardLayout::parse(&raw).unwrap();
        assert!(reloaded.widgets[0].size.w <= 12);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
