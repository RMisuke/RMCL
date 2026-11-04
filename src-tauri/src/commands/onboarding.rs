//! First-run onboarding.
//!
//! The wizard is offered when neither the per-user root nor the portable root
//! contains a launcher config. Detection runs before the core initialises (a
//! snapshot on `AppState`), so folders the core later creates for its own
//! startup cannot turn a genuine first run into a returning one.

use rmccc_config::paths::{self, DataMode};
use rmccc_error::{ErrorCode, IpcError};
use serde::Serialize;
use ts_rs::TS;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct LauncherConfigView {
    /// `hmcl` or `pcl`.
    pub launcher: String,
    pub config_path: String,
    /// Custom directory name from the other launcher, if any.
    pub name: String,
    /// Game directory to register; empty when the config lists none.
    pub directory: String,
    pub instance_count: usize,
    /// The other launcher currently has this directory selected.
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct OnboardingProbeView {
    pub needs_onboarding: bool,
    pub mode: String,
    pub executable_dir: String,
    pub portable_root: String,
    pub standard_root: String,
    pub portable_minecraft: String,
    /// `.minecraft` existed before the core created anything this session.
    pub minecraft_existed: bool,
    pub minecraft_instances: usize,
    pub configs: Vec<LauncherConfigView>,
}

/// Startup snapshot of the portable layout. Captured before core init.
#[derive(Debug, Clone)]
pub struct OnboardingSnapshot {
    /// Captured before the core (or the frontend) writes anything.
    pub needs_onboarding: bool,
    pub mode: String,
    pub executable_dir: String,
    pub portable_root: String,
    pub standard_root: String,
    pub portable_minecraft: String,
    pub minecraft_existed: bool,
    pub minecraft_instances: usize,
    pub configs: Vec<rmccc_instance::compat::LauncherConfigHit>,
}

impl OnboardingSnapshot {
    pub fn detect() -> Self {
        let mode = paths::AppPaths::current().mode();
        let executable_dir = paths::executable_dir();
        let minecraft = executable_dir.join(".minecraft");
        let configs = rmccc_instance::compat::detect_launcher_configs(&executable_dir, &minecraft);
        Self {
            needs_onboarding: !paths::has_any_launcher_config(),
            mode: mode.as_str().to_string(),
            executable_dir: executable_dir.display().to_string(),
            portable_root: paths::data_dir_for_mode(DataMode::Portable)
                .display()
                .to_string(),
            standard_root: paths::standard_data_dir().display().to_string(),
            portable_minecraft: minecraft.display().to_string(),
            minecraft_existed: minecraft.is_dir(),
            minecraft_instances: rmccc_instance::compat::count_instances(&minecraft),
            configs,
        }
    }

    pub fn view(&self) -> OnboardingProbeView {
        OnboardingProbeView {
            needs_onboarding: self.needs_onboarding,
            mode: self.mode.clone(),
            executable_dir: self.executable_dir.clone(),
            portable_root: self.portable_root.clone(),
            standard_root: self.standard_root.clone(),
            portable_minecraft: self.portable_minecraft.clone(),
            minecraft_existed: self.minecraft_existed,
            minecraft_instances: self.minecraft_instances,
            configs: self
                .configs
                .iter()
                .map(|hit| LauncherConfigView {
                    launcher: hit.launcher.as_str().to_string(),
                    config_path: hit.config_path.display().to_string(),
                    name: hit.name.clone().unwrap_or_default(),
                    directory: hit
                        .game_dir
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    instance_count: hit.instance_count,
                    selected: hit.selected,
                })
                .collect(),
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub fn onboarding_probe(state: tauri::State<'_, AppState>) -> OnboardingProbeView {
    state.onboarding.view()
}

/// Finish the wizard and mark the selected data root as configured.
///
/// Switching to portable mode is a separate `data_mode_switch` call (it
/// restarts the process); by the time the wizard finishes, the current mode is
/// expected to equal `mode`.
#[tauri::command]
#[tracing::instrument]
pub fn onboarding_complete(mode: String) -> Result<(), IpcError> {
    let mode = DataMode::parse(&mode).ok_or_else(|| IpcError {
        code: ErrorCode::ValidationError,
        message: format!("unknown data mode: {mode}"),
        retryable: false,
    })?;
    let target = paths::data_dir_for_mode(mode);
    paths::mark_onboarded(&target).map_err(io_error)?;
    if paths::AppPaths::current().mode() == mode {
        paths::ensure_dirs().map_err(io_error)?;
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> IpcError {
    IpcError {
        code: ErrorCode::Unknown,
        message: error.to_string(),
        retryable: false,
    }
}
