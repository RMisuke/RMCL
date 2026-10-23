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

use crate::state::AppState;
use rmccc_agent::{
    AgentRoots, AgentService, AgentServiceConfig, ApprovalBroker, CancelToken, CredentialStore,
    FileCredentialStore, OpenAiCompatibleProvider, SessionStore, ToolRegistry,
};
use rmccc_error::{ErrorCode, IpcError};
use serde::Serialize;
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

/// 打开（或首次创建）Agent 审计日志并挂到注册表上。
fn audited_registry(store: &SessionStore, roots: &AgentRoots) -> rmccc_error::Result<ToolRegistry> {
    let audit = store.open_audit()?;
    Ok(rmccc_agent::read_only_registry(roots)?.with_audit(Arc::new(audit)))
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

    let (store, registry, model_name) = {
        let core = state.core.read().await;
        let store = {
            let data_dir = core.config.resolved_data_dir();
            SessionStore::open(&data_dir, core.platform.fs.clone())?
        };
        let roots = agent_roots(&core, profile_id.as_deref())?;
        let registry = audited_registry(&store, &roots)?;
        let model_name = credential.model.clone().unwrap_or_default();
        (Arc::new(store), Arc::new(registry), model_name)
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
        .run_turn(&message, profile_id.as_deref(), cancel)
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
