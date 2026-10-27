//! AI 服务 IPC 命令（ADR-013 S1/S2：OpenAI 兼容端口，一次配置全局使用）。
//!
//! | 命令 | 说明 |
//! |------|------|
//! | [`ai_config_get`] / [`ai_config_set`] | 端点 / 模型 / 密钥（密钥入 vault，视图永不回传） |
//! | [`ai_models_probe`] | `GET {base}/models` 自动检测可用模型（保存前即可探测） |
//! | [`ai_analyze_log`] / [`ai_analyze_cancel`] | 游戏日志一键分析（流式事件 `ai://delta/done/error`） |
//!
//! 迁移（ADR-013）：接替 `agent_config_get/set`——沿用同一 vault 键 `llm`，
//! 旧配置无缝继承；内建 Agent 执行循环 / 会话 / 审批已退役。
//!
//! 数据外发边界：`ai_analyze_log` 只发送**脱敏后**的日志尾部（home → `~`、
//! 凭据形状遮蔽），返回值回传发送行数保持对用户透明。

use crate::state::AppState;
use rmccc_error::{ErrorCode, IpcError, RmcccError};
use rmccc_llm::credentials::{validate_endpoint, CredentialStore as _};
use rmccc_llm::redact;
use rmccc_llm::{
    llm::{
        probe_models, ChatMessage, ChatRequest, LlmEvent, LlmOptions, LlmProvider,
        OpenAiCompatibleProvider,
    },
    CancelToken, FileCredentialStore, ServiceCredential,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;

/// AI 服务在凭据 vault 里的键名（ADR-009；沿用 agent 时代的 `llm`，配置无缝迁移）。
const AI_SERVICE: &str = "llm";
/// 单次分析最多发送的日志字符数（尾部截取）。
const ANALYSIS_LOG_MAX_CHARS: usize = 12_000;
/// 并发分析上限（防狂点）。
const MAX_CONCURRENT_ANALYSES: usize = 2;
/// 单次分析整体超时（含流式读取）。
const ANALYSIS_TIMEOUT_SECS: u64 = 180;

const ANALYSIS_SYSTEM_PROMPT: &str = "你是 Minecraft 启动器（RMCCC）的日志分析助手。\
用户提供一次游戏运行的日志片段（已脱敏）。请：\
1) 指出最可能导致故障的原因，引用日志中的关键行；\
2) 给出 2-3 条可执行的排查或修复建议；\
3) 如果日志不足以判断，说明还需要什么信息。\
用简体中文回答，简洁分点，不要编造日志中不存在的内容。";

/// AI 配置的公开视图：永远不含 api_key 本体。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct AiConfigView {
    pub configured: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub has_key: bool,
}

/// `ai_analyze_log` 的返回：分析句柄 + 透明性信息。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct AiAnalysisHandle {
    pub analysis_id: String,
    /// 本次发送给模型的脱敏日志行数。
    pub sent_lines: usize,
}

async fn credential_store(state: &AppState) -> rmccc_error::Result<FileCredentialStore> {
    let (data_dir, fs) = {
        let core = state.core.read().await;
        (core.config.resolved_data_dir(), core.platform.fs.clone())
    };
    FileCredentialStore::open(&data_dir, fs)
}

/// 宿主 AI 服务的凭据（service=`llm`）。S6 插件桥 `llm.chat` 复用：
/// 插件用宿主配置的端点与密钥，永远拿不到 key 本体（ADR-013 §4.3）。
pub(crate) async fn host_llm_credential(
    state: &AppState,
) -> rmccc_error::Result<ServiceCredential> {
    load_credential(state).await
}

async fn load_credential(state: &AppState) -> rmccc_error::Result<ServiceCredential> {
    credential_store(state)
        .await?
        .get(AI_SERVICE)?
        .ok_or_else(|| {
            RmcccError::Validation("AI 服务未配置：请先在设置中填写 API 地址 / 模型 / 密钥".into())
        })
}

// ── 配置 ──

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_config_get(state: tauri::State<'_, AppState>) -> Result<AiConfigView, IpcError> {
    let credential = credential_store(&state).await?.get(AI_SERVICE)?;
    Ok(match credential {
        Some(credential) => AiConfigView {
            configured: true,
            base_url: credential.base_url,
            model: credential.model,
            has_key: !credential.api_key.is_empty(),
        },
        None => AiConfigView {
            configured: false,
            base_url: None,
            model: None,
            has_key: false,
        },
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_config_set(
    state: tauri::State<'_, AppState>,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> Result<(), IpcError> {
    // 字段可选（ADR-013 §4.2）：留空/缺省 = 沿用已存值，方便只换模型不重输密钥。
    let store = credential_store(&state).await?;
    let existing = store.get(AI_SERVICE)?;
    let credential = merge_credential(existing.as_ref(), base_url, model, api_key)?;
    store.put(credential)?;
    Ok(())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 将一次部分更新合并到已存凭据上；三要素（端点/模型/密钥）最终必须齐备。
fn merge_credential(
    existing: Option<&ServiceCredential>,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> rmccc_error::Result<ServiceCredential> {
    let fallback = ServiceCredential {
        service: AI_SERVICE.to_string(),
        api_key: String::new(),
        base_url: None,
        model: None,
    };
    let keep = existing.unwrap_or(&fallback);
    let base = non_empty(base_url).or_else(|| keep.base_url.clone());
    let model = non_empty(model).or_else(|| keep.model.clone());
    let key = non_empty(api_key).unwrap_or_else(|| keep.api_key.clone());
    let (base, model) = match (base, model) {
        (Some(base), Some(model)) => (base, model),
        _ => {
            return Err(RmcccError::Validation(
                "AI 配置不完整：首次保存需同时提供 API 地址、模型与密钥".into(),
            ));
        }
    };
    if key.trim().is_empty() {
        return Err(RmcccError::Validation(
            "AI 配置不完整：首次保存需同时提供 API 地址、模型与密钥".into(),
        ));
    }
    ServiceCredential::new(AI_SERVICE, key)?
        .with_endpoint(base)?
        .with_model(model)
}

/// 模型探测：传了 `base_url` 就用传入值（保存前探测），否则用已存配置。
#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_models_probe(
    state: tauri::State<'_, AppState>,
    base_url: Option<String>,
    api_key: Option<String>,
) -> Result<Vec<String>, IpcError> {
    let (base, key) = match base_url {
        Some(base) if !base.trim().is_empty() => (base, api_key.unwrap_or_default()),
        _ => {
            let credential = load_credential(&state).await?;
            (credential.base_url.unwrap_or_default(), credential.api_key)
        }
    };
    if let Err(error) = validate_endpoint(&base) {
        return Err(IpcError::from(error));
    }
    probe_models(&base, &key)
        .await
        .map_err(|e| IpcError::from(RmcccError::from(e)))
}

// ── 日志一键分析（S2 内置参考消费方）──

/// 收集一次分析的输入：run_dir/logs/latest.log 尾部（脱敏）+ 实例上下文行。
async fn gather_analysis_input(
    state: &AppState,
    instance_key: Option<&str>,
) -> rmccc_error::Result<(String, String, usize)> {
    let core = state.core.read().await;
    let (log_path, context) = match instance_key {
        Some(key) => {
            let detail = {
                let mut service = core
                    .instances
                    .lock()
                    .map_err(|_| RmcccError::Other("Instance service lock poisoned".to_string()))?;
                service.get(key)?
            };
            let loader = detail
                .summary
                .loader
                .as_ref()
                .map(|loader| loader.kind.clone())
                .unwrap_or_else(|| "无".into());
            (
                detail.paths.logs_dir.join("latest.log"),
                format!(
                    "实例「{}」：Minecraft {}，加载器 {}",
                    detail.summary.display_name, detail.summary.game_version, loader
                ),
            )
        }
        None => (
            core.config
                .resolved_game_root()
                .join("logs")
                .join("latest.log"),
            "共享游戏目录（未指定实例）".into(),
        ),
    };
    let log_path = log_path.clone();
    let captured_key = instance_key.map(str::to_string);
    drop(core);

    // Perf audit 1.3: read only a tail window off-thread — the full
    // latest.log (tens of MB after a crash) was previously buffered whole on
    // the async worker just to slice `ANALYSIS_LOG_MAX_CHARS` from the end.
    let read_tail = {
        let log_path = log_path.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<Option<String>> {
            use std::io::{Read, Seek, SeekFrom};
            let Ok(mut file) = std::fs::File::open(&log_path) else {
                return Ok(None);
            };
            let len = file.metadata()?.len();
            // Over-read ×3 to cover multi-byte UTF-8; exact truncation to
            // ANALYSIS_LOG_MAX_CHARS happens below on the string.
            let want = (ANALYSIS_LOG_MAX_CHARS as u64).saturating_mul(3).min(len);
            if want > 0 {
                file.seek(SeekFrom::End(-(want as i64)))?;
            }
            let mut buf = Vec::with_capacity(want as usize);
            file.read_to_end(&mut buf)?;
            Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
        })
        .await
        .map_err(|e| RmcccError::Other(format!("log tail read join error: {e}")))?
    };
    let (raw, source) = match read_tail {
        Ok(Some(text)) if !text.trim().is_empty() => (text, "游戏日志 latest.log"),
        Err(error) => {
            return Err(RmcccError::Io(error));
        }
        // latest.log 未生成（log4j 初始化前崩溃，如 BootstrapLauncher 阶段的
        // ExceptionInInitializerError）或为空 → 回退启动器捕获的进程输出。
        _ => {
            let captured = captured_key
                .as_ref()
                .and_then(|key| state.game_output.get(key))
                .map(|buf| {
                    buf.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .join("\n")
                });
            match captured {
                Some(text) if !text.trim().is_empty() => {
                    (text, "启动器捕获的进程输出（latest.log 未生成或为空）")
                }
                _ => {
                    return Err(RmcccError::Validation(format!(
                        "未找到游戏日志（{}），也没有捕获到进程输出：先启动一次游戏再分析",
                        log_path.display()
                    )));
                }
            }
        }
    };
    // 尾部截取（崩溃信息几乎总在末尾），再脱敏（home → ~、凭据遮蔽）。
    let tail = tail_chars(&raw, ANALYSIS_LOG_MAX_CHARS);
    let sent_lines = tail.lines().count();
    let redacted = redact::redact_text(tail, redact::user_home().as_deref());
    let context = format!("{context}；日志来源：{source}");
    Ok((redacted, context, sent_lines))
}

fn tail_chars(text: &str, max_chars: usize) -> &str {
    let total = text.chars().count();
    if total <= max_chars {
        return text;
    }
    let start = text
        .char_indices()
        .nth(total - max_chars)
        .map(|(index, _)| index)
        .unwrap_or(0);
    text[start..].trim_start_matches('\n')
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_analyze_log(
    state: tauri::State<'_, AppState>,
    key: Option<String>,
) -> Result<AiAnalysisHandle, IpcError> {
    let credential = load_credential(&state).await?;
    let (log_text, context, sent_lines) = gather_analysis_input(&state, key.as_deref()).await?;

    let analysis_id = uuid::Uuid::new_v4().simple().to_string();
    let cancel = CancelToken::new();
    {
        let mut cancels = state
            .ai_cancels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cancels.len() >= MAX_CONCURRENT_ANALYSES {
            return Err(IpcError {
                code: ErrorCode::AlreadyRunning,
                message: "too many concurrent AI analyses".into(),
                retryable: false,
            });
        }
        cancels.insert(analysis_id.clone(), cancel.clone());
    }

    let app = state.app_handle.clone();
    let cancels = state.ai_cancels.clone();
    let task_analysis_id = analysis_id.clone();
    tauri::async_runtime::spawn(async move {
        run_analysis(
            app,
            cancels,
            task_analysis_id,
            credential,
            context,
            log_text,
            cancel,
        )
        .await;
    });

    Ok(AiAnalysisHandle {
        analysis_id,
        sent_lines,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_analysis(
    app: tauri::AppHandle,
    cancels: Arc<std::sync::Mutex<HashMap<String, CancelToken>>>,
    analysis_id: String,
    credential: ServiceCredential,
    context: String,
    log_text: String,
    cancel: CancelToken,
) {
    use tauri::Emitter;

    let provider = match OpenAiCompatibleProvider::new(
        &credential,
        LlmOptions {
            timeout: Duration::from_secs(ANALYSIS_TIMEOUT_SECS),
            ..Default::default()
        },
    ) {
        Ok(provider) => provider,
        Err(error) => {
            emit_ai_error(&app, &analysis_id, &error.to_string());
            remove_cancel(&cancels, &analysis_id);
            return;
        }
    };
    let request = ChatRequest::new(vec![
        ChatMessage::system(ANALYSIS_SYSTEM_PROMPT),
        ChatMessage::user(format!("{context}\n\n----- 日志（脱敏）-----\n{log_text}")),
    ]);
    let mut stream = match provider.stream_chat(request, cancel) {
        Ok(stream) => stream,
        Err(error) => {
            emit_ai_error(&app, &analysis_id, &error.to_string());
            remove_cancel(&cancels, &analysis_id);
            return;
        }
    };

    while let Some(event) = stream.recv().await {
        match event {
            LlmEvent::Delta(text) => {
                let _ = app.emit(
                    crate::events::AI_DELTA,
                    &crate::events::AiDeltaPayload {
                        analysis_id: analysis_id.clone(),
                        text,
                    },
                );
            }
            LlmEvent::Usage { .. } => {}
            LlmEvent::Done { .. } => {
                let _ = app.emit(
                    crate::events::AI_DONE,
                    &crate::events::AiDonePayload {
                        analysis_id: analysis_id.clone(),
                    },
                );
                break;
            }
            LlmEvent::Error(error) => {
                emit_ai_error(&app, &analysis_id, &error.to_string());
                break;
            }
        }
    }
    remove_cancel(&cancels, &analysis_id);
}

fn emit_ai_error(app: &tauri::AppHandle, analysis_id: &str, message: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        crate::events::AI_ERROR,
        &crate::events::AiErrorPayload {
            analysis_id: analysis_id.to_string(),
            message: message.to_string(),
        },
    );
}

fn remove_cancel(cancels: &Arc<std::sync::Mutex<HashMap<String, CancelToken>>>, analysis_id: &str) {
    let mut cancels = cancels
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancels.remove(analysis_id);
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_analyze_cancel(
    state: tauri::State<'_, AppState>,
    analysis_id: String,
) -> Result<bool, IpcError> {
    let mut cancels = state
        .ai_cancels
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(cancels.remove(&analysis_id).is_some_and(|token| {
        token.cancel();
        true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_chars_keeps_the_tail_and_cuts_on_char_boundary() {
        let text = "abcdefghij";
        assert_eq!(tail_chars(text, 20), text);
        let tail = tail_chars(text, 4);
        assert_eq!(tail, "ghij");
        // 多字节字符不被截半。
        let cjk = "一二三四五";
        assert_eq!(tail_chars(cjk, 2), "四五");
    }

    fn stored_credential() -> ServiceCredential {
        ServiceCredential {
            service: AI_SERVICE.to_string(),
            api_key: "old-key".into(),
            base_url: Some("https://api.example.com/v1".into()),
            model: Some("old-model".into()),
        }
    }

    #[test]
    fn merge_credential_fills_missing_fields_from_existing() {
        // 只换模型：端点与密钥留空 → 沿用已存值。
        let merged = merge_credential(
            Some(&stored_credential()),
            None,
            Some("new-model".into()),
            None,
        )
        .expect("partial update keeps stored endpoint and key");
        assert_eq!(
            merged.base_url.as_deref(),
            Some("https://api.example.com/v1")
        );
        assert_eq!(merged.model.as_deref(), Some("new-model"));
        assert_eq!(merged.api_key, "old-key");
    }

    #[test]
    fn merge_credential_ignores_blank_strings() {
        let merged = merge_credential(
            Some(&stored_credential()),
            Some("   ".into()),
            Some("  m2  ".into()),
            Some("".into()),
        )
        .expect("blank inputs fall back to stored values");
        assert_eq!(merged.model.as_deref(), Some("m2"));
        assert_eq!(merged.api_key, "old-key");
    }

    #[test]
    fn merge_credential_requires_full_config_when_nothing_stored() {
        let error = merge_credential(None, None, None, None)
            .expect_err("empty merge without a stored credential must fail");
        assert!(matches!(error, RmcccError::Validation(_)));
    }
}
