//! Agent 平台 IPC 命令（Phase 6.5，ADR-009/010）。
//!
//! | 命令 | 说明 |
//! |------|------|
//! | [`agent_config_get`] / [`agent_config_set`] | LLM 服务凭据（公开视图无 `api_key`） |
//! | [`agent_session_list`] / [`agent_session_read`] | 会话事件流（只读） |
//! | [`agent_tasks_pending`] / [`agent_task_resolve`] | 审批队列与用户决断 |
//! | [`agent_run`] / [`agent_cancel`] | 执行回合（同一时刻最多一个）/ 取消 |
//! | [`agent_clear_history`] | 清除会话与任务（保留审计） |
//!
//! 安全边界与 `rmccc-agent` 一致：模型输出不是指令；工具调用全部过
//! `ToolRegistry`（Schema 校验 → 风险门禁 → 审计）；本批只放行只读工具。

use crate::commands::mods::sanitize_filename;
use crate::state::AppState;
use rmccc_agent::{
    AgentRoots, AgentService, AgentServiceConfig, ApprovalBroker, CancelToken, CredentialStore,
    FileCredentialStore, OpenAiCompatibleProvider, SessionStore,
};
use rmccc_error::{ErrorCode, IpcError};
use rmccc_mods::ModSource as _;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// LLM 服务在凭据 vault 里的键名（ADR-009）。
const AGENT_SERVICE: &str = "llm";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigView {
    pub configured: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunOutput {
    pub session_id: String,
    pub status: String,
    pub replies: Vec<String>,
    pub tool_calls: u32,
    /// 本回合实际发给远程模型的上下文（已脱敏，6.7）。
    pub context_sent: Option<Value>,
}

async fn credential_store(state: &AppState) -> rmccc_error::Result<FileCredentialStore> {
    let (data_dir, fs) = {
        let core = state.core.read().await;
        (core.config.resolved_data_dir(), core.platform.fs.clone())
    };
    FileCredentialStore::open(&data_dir, fs)
}

fn agent_roots(
    core: &rmccc_core::CoreContext,
    profile_id: Option<&str>,
) -> rmccc_error::Result<AgentRoots> {
    let game_root = core.config.resolved_game_root();
    let versions_dir = game_root.join("versions");
    let mods_dir = match profile_id {
        Some(id) => {
            let profile = core
                .profile_store
                .get(id)?
                .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(id.to_string()))?;
            rmccc_launch::instance_mods_dir(&core.config, &profile)
        }
        None => game_root.join("mods"),
    };
    Ok(AgentRoots {
        versions_dir,
        mods_dir,
    })
}

async fn session_store(state: &AppState) -> rmccc_error::Result<Arc<SessionStore>> {
    let (data_dir, fs) = {
        let core = state.core.read().await;
        (core.config.resolved_data_dir(), core.platform.fs.clone())
    };
    SessionStore::open(&data_dir, fs).map(Arc::new)
}

/// 指南 §10.6 的计划执行器：Modrinth 解析 + 下载安装（SHA1 校验由
/// `ModSource::install` 负责）。同步 trait 经 `block_in_place` 桥接。
struct ModrinthPlanApplier {
    source: rmccc_mods::modrinth::ModrinthSource,
    game_version: String,
    loader: Option<String>,
}

impl ModrinthPlanApplier {
    async fn apply_async(
        &self,
        mods_dir: &std::path::Path,
        dependencies: &[rmccc_agent::PlannedDependency],
    ) -> rmccc_error::Result<rmccc_agent::AppliedPlanReport> {
        let mut installed = Vec::new();
        let mut failed = Vec::new();
        let mut attempted = Vec::new();
        for dep in dependencies {
            attempted.push(dep.mod_id.clone());
            match self.install_one(mods_dir, dep).await {
                Ok(applied) => installed.push(applied),
                Err(error) => failed.push(rmccc_agent::FailedMod {
                    mod_id: dep.mod_id.clone(),
                    reason: error.to_string(),
                }),
            }
        }
        Ok(rmccc_agent::AppliedPlanReport {
            installed,
            failed,
            attempted,
        })
    }

    async fn install_one(
        &self,
        mods_dir: &std::path::Path,
        dep: &rmccc_agent::PlannedDependency,
    ) -> rmccc_error::Result<rmccc_agent::AppliedMod> {
        let search = self
            .source
            .search(&rmccc_mods::ModSearchQuery {
                query: dep.mod_id.clone(),
                game_version: (!self.game_version.is_empty()).then(|| self.game_version.clone()),
                loader: self.loader.clone(),
                page: 0,
                limit: 10,
            })
            .await?;
        let hit = search.hits.first().ok_or_else(|| {
            rmccc_error::RmcccError::MissingDependency(format!(
                "no Modrinth project matches '{}'",
                dep.mod_id
            ))
        })?;
        let versions = self.source.list_versions(&hit.id).await?;
        // 优先"游戏版本 + 加载器"都匹配，其次只匹配游戏版本，最后取最新。
        let version = versions
            .iter()
            .find(|v| {
                v.game_versions.iter().any(|g| g == &self.game_version)
                    && self
                        .loader
                        .as_ref()
                        .is_some_and(|l| v.loaders.iter().any(|v_l| v_l == l))
            })
            .or_else(|| {
                versions
                    .iter()
                    .find(|v| v.game_versions.iter().any(|g| g == &self.game_version))
            })
            .or_else(|| versions.first())
            .ok_or_else(|| {
                rmccc_error::RmcccError::MissingDependency(format!(
                    "no downloadable version for '{}' ({})",
                    dep.mod_id, hit.id
                ))
            })?
            .clone();

        // 约束字符串（如 ">=1.0"）以源元数据记录为准，由用户在审批载荷里
        // 人工确认——本版本不做 semver 求解。
        let raw_name = version
            .filename
            .clone()
            .unwrap_or_else(|| format!("{}-{}.jar", dep.mod_id, version.version_number));
        let filename = sanitize_filename(&raw_name);
        let dest = mods_dir.join(filename);
        self.source.install(&version, &dest).await?;
        Ok(rmccc_agent::AppliedMod {
            mod_id: dep.mod_id.clone(),
            file: dest
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            version: Some(version.version_number.clone()),
        })
    }
}

impl rmccc_agent::PlanApplier for ModrinthPlanApplier {
    fn apply(
        &self,
        mods_dir: &std::path::Path,
        dependencies: &[rmccc_agent::PlannedDependency],
    ) -> rmccc_error::Result<rmccc_agent::AppliedPlanReport> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.apply_async(mods_dir, dependencies))
        })
    }
}

/// 6.7 上下文构建：只取**所选 profile** 的元数据（ADR-010 决策 5），
/// 不读日志 / 存档 / 账号数据；runtime 在进提示词前统一脱敏。
fn agent_context(
    core: &rmccc_core::CoreContext,
    profile_id: Option<&str>,
) -> rmccc_error::Result<Option<Value>> {
    let Some(id) = profile_id else {
        return Ok(None);
    };
    let profile = core
        .profile_store
        .get(id)?
        .ok_or_else(|| rmccc_error::RmcccError::ProfileNotFound(id.to_string()))?;
    Ok(Some(json!({
        "profile_id": profile.id,
        "profile_name": profile.name,
        "game_version": profile.game_version,
        "mod_loader": profile.mod_loader.as_ref().map(|loader| json!({
            "type": format!("{:?}", loader.loader_type).to_lowercase(),
            "version": loader.loader_version,
        })),
        "isolation": profile.isolation,
        "memory_mb": profile.memory_mb,
    })))
}

// ── 配置 ──

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_config_get(
    state: tauri::State<'_, AppState>,
) -> Result<AgentConfigView, IpcError> {
    let credential = credential_store(&state).await?.get(AGENT_SERVICE)?;
    Ok(match credential {
        Some(credential) => AgentConfigView {
            configured: true,
            base_url: credential.base_url,
            model: credential.model,
        },
        None => AgentConfigView {
            configured: false,
            base_url: None,
            model: None,
        },
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_config_set(
    state: tauri::State<'_, AppState>,
    base_url: String,
    model: String,
    api_key: String,
) -> Result<(), IpcError> {
    let credential = rmccc_agent::ServiceCredential::new(AGENT_SERVICE, api_key)?
        .with_endpoint(base_url)?
        .with_model(model)?;
    credential_store(&state).await?.put(credential)?;
    Ok(())
}

// ── 会话与审批 ──

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_session_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<rmccc_agent::SessionSummary>, IpcError> {
    session_store(&state).await?.list().map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_session_read(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<Vec<rmccc_agent::SessionEvent>, IpcError> {
    session_store(&state)
        .await?
        .read(&session_id)
        .map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_tasks_pending(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<rmccc_agent::TaskSnapshot>, IpcError> {
    let store = session_store(&state).await?;
    ApprovalBroker::new(store).pending().map_err(IpcError::from)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_task_resolve(
    state: tauri::State<'_, AppState>,
    task_id: String,
    approved: bool,
    reason: Option<String>,
) -> Result<rmccc_agent::TaskSnapshot, IpcError> {
    let store = session_store(&state).await?;
    ApprovalBroker::new(store)
        .resolve(&task_id, approved, reason.as_deref())
        .map_err(IpcError::from)
}

// ── 回合执行 ──

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_run(
    state: tauri::State<'_, AppState>,
    message: String,
    profile_id: Option<String>,
) -> Result<AgentRunOutput, IpcError> {
    if message.trim().is_empty() {
        return Err(IpcError {
            code: ErrorCode::ValidationError,
            message: "message must not be empty".into(),
            retryable: false,
        });
    }

    // 同一时刻最多一个回合：抢到取消槽才继续。
    let cancel = CancelToken::new();
    {
        let mut slot = state
            .agent_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            return Err(IpcError {
                code: ErrorCode::AlreadyRunning,
                message: "an agent turn is already running".into(),
                retryable: false,
            });
        }
        *slot = Some(cancel.clone());
    }

    let result = run_turn_locked(&state, message, profile_id, cancel).await;

    {
        let mut slot = state
            .agent_cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = None;
    }

    let output = result?;
    Ok(AgentRunOutput {
        session_id: output.session_id,
        status: format!("{:?}", output.status).to_lowercase(),
        replies: output.replies,
        tool_calls: output.tool_calls as u32,
        context_sent: output.context_sent,
    })
}

async fn run_turn_locked(
    state: &tauri::State<'_, AppState>,
    message: String,
    profile_id: Option<String>,
    cancel: CancelToken,
) -> rmccc_error::Result<rmccc_agent::TurnOutput> {
    let credential = credential_store(state)
        .await?
        .get(AGENT_SERVICE)?
        .ok_or_else(|| {
            rmccc_error::RmcccError::Validation(
                "agent is not configured: set base_url / model / api_key first".into(),
            )
        })?;

    let (store, registry, model_name, context) = {
        let core = state.core.read().await;
        let store = {
            let data_dir = core.config.resolved_data_dir();
            SessionStore::open(&data_dir, core.platform.fs.clone())?
        };
        let roots = agent_roots(&core, profile_id.as_deref())?;
        // 完整工具集 = 只读工具 + mods.apply_plan（写工具，走审批链）。
        let applier = ModrinthPlanApplier {
            source: rmccc_mods::modrinth::ModrinthSource::new(core.http_client.clone()),
            game_version: profile_id
                .as_deref()
                .and_then(|id| core.profile_store.get(id).ok().flatten())
                .map(|p| p.game_version)
                .unwrap_or_default(),
            loader: profile_id
                .as_deref()
                .and_then(|id| core.profile_store.get(id).ok().flatten())
                .and_then(|p| p.mod_loader)
                .map(|loader| format!("{:?}", loader.loader_type).to_lowercase()),
        };
        let registry = rmccc_agent::full_registry(&roots, Arc::new(applier))?
            .with_audit(Arc::new(store.open_audit()?));
        let context = agent_context(&core, profile_id.as_deref())?;
        let model_name = credential.model.clone().unwrap_or_default();
        (Arc::new(store), Arc::new(registry), model_name, context)
    };

    let provider = OpenAiCompatibleProvider::new(
        &credential,
        rmccc_agent::llm::LlmOptions {
            timeout: Duration::from_secs(120),
            ..Default::default()
        },
    )
    .map_err(|e| rmccc_error::RmcccError::Other(format!("agent provider: {e}")))?;

    let service = AgentService::new(
        store,
        registry,
        Arc::new(provider),
        AgentServiceConfig {
            model: model_name,
            ..Default::default()
        },
    );
    service
        .run_turn_with_context(&message, profile_id.as_deref(), context, cancel)
        .await
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_cancel(state: tauri::State<'_, AppState>) -> Result<bool, IpcError> {
    let mut slot = state
        .agent_cancel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(slot.take().is_some_and(|token| {
        token.cancel();
        true
    }))
}

// ── 历史清理 ──

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn agent_clear_history(
    state: tauri::State<'_, AppState>,
) -> Result<rmccc_agent::PurgeReport, IpcError> {
    session_store(&state)
        .await?
        .clear_all()
        .map_err(IpcError::from)
}
