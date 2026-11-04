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
//! 数据外发边界：`ai_analyze_log` 只发送**脱敏后**的日志（home → `~`、
//! 凭据形状遮蔽），返回值回传发送行数保持对用户透明。
//!
//! # S2 重构要点（见 `docs/AI_ANALYSIS_REFACTOR.md`）
//!
//! - **运行绑定**：分析输入经 `game_log::resolve_fresh_log` 绑定到本轮运行
//!   （captured 优先，磁盘仅当 mtime ≥ 本次启动），不再读到上一轮的 `latest.log`。
//! - **终止保证**：`Done` / `Cancelled` / `Error` 都会 emit 终止事件；取消不再
//!   让前端永久停在"分析中"。
//! - **提交模式**：标准＝完整日志尾部；激进＝仅 ERROR/WARNING 行（省 token）。
//! - **详细程度**：简略 / 适中 / 详细三档，决定提示词与输出上限。
//! - **跟随界面语言**：系统/用户提示按 zh-CN / en 生成。

use crate::game_log::{self, LogSourceKind};
use crate::state::{AnalysisRecord, AppState};
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
/// 标准模式：磁盘尾读 + 发送窗口（完整日志）。
const ANALYSIS_STANDARD_WINDOW_CHARS: usize = 24_000;
/// 激进模式：先扫更大窗口再筛 ERROR/WARNING 行。
const ANALYSIS_AGGRESSIVE_SCAN_CHARS: usize = 120_000;
/// 激进模式：筛选后发送上限。
const ANALYSIS_AGGRESSIVE_MAX_CHARS: usize = 6_000;
/// 并发分析上限（防狂点）。
const MAX_CONCURRENT_ANALYSES: usize = 2;
/// 单次分析整体超时（含流式读取）。
const ANALYSIS_TIMEOUT_SECS: u64 = 120;
/// 首字节超时（连接已建立但模型不吐字）。
const ANALYSIS_FIRST_BYTE_TIMEOUT_SECS: u64 = 30;
/// 诊断任务用低温度，输出稳定、克制。
const ANALYSIS_TEMPERATURE: f32 = 0.2;

/// 日志提交模式（用户可在「设置 · AI」切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogMode {
    /// 完整日志尾部。
    Standard,
    /// 仅 ERROR / WARNING（含 FATAL）行，最大程度省 token。
    Aggressive,
}

impl LogMode {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("aggressive") => LogMode::Aggressive,
            _ => LogMode::Standard,
        }
    }
}

/// 输出详细程度（用户可在「设置 · AI」切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Detail {
    Brief,
    Balanced,
    Detailed,
}

impl Detail {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("brief") => Detail::Brief,
            Some("detailed") => Detail::Detailed,
            _ => Detail::Balanced,
        }
    }

    fn max_tokens(self) -> u32 {
        match self {
            Detail::Brief => 500,
            Detail::Balanced => 1_000,
            Detail::Detailed => 1_800,
        }
    }
}

// ── 系统提示（按语言 × 详细程度）──
//
// 统一约束：只依据日志、禁止编造、禁止复述；固定三段（结论/关键行/建议）。

const SYSTEM_PROMPT_ZH_BRIEF: &str = "你是 Minecraft 启动器的日志诊断助手。只依据用户提供的脱敏日志作答，禁止编造或推测日志中不存在的内容，禁止复述日志原文。用简体中文，总字数不超过 200 字，严格输出三段，不加前言、结语或额外标题：\n结论：一句话指出根因；日志不足时写“日志不足”并说明还缺什么。\n关键行：最多 2 条，每条引用日志片段（不超过 20 字）。\n建议：最多 2 条可执行步骤，每条一句话。";

const SYSTEM_PROMPT_ZH_BALANCED: &str = "你是 Minecraft 启动器的日志诊断助手。只依据用户提供的脱敏日志作答，禁止编造或推测日志中不存在的内容，禁止复述日志原文。用简体中文，总字数 300–400 字，严格输出三段，不加前言、结语或额外标题：\n结论：2 句以内指出根因；日志不足时写“日志不足”并说明还缺什么。\n关键行：最多 3 条，每条引用日志片段（不超过 20 字）。\n建议：最多 3 条可执行修复步骤，每条一句话；涉及配置或命令时给出具体项。";

const SYSTEM_PROMPT_ZH_DETAILED: &str = "你是 Minecraft 启动器的日志诊断助手。只依据用户提供的脱敏日志作答，禁止编造或推测日志中不存在的内容，禁止复述日志原文。用简体中文，总字数 600–800 字，严格输出三段，不加前言、结语或额外标题：\n结论：说明根因与触发条件。\n关键行：最多 5 条，每条引用日志片段（不超过 20 字）。\n建议：按优先级给出尽量具体的修复步骤（版本、加载器、配置文件、命令）；缺少信息时明确说明如何获取。";

const SYSTEM_PROMPT_EN_BRIEF: &str = "You are the log-diagnostics assistant of a Minecraft launcher. Answer using ONLY the redacted log the user provides. Never invent or infer facts absent from the log, and never restate the log verbatim. Reply in English, at most 200 words, in exactly these three sections, with no preamble or closing:\nConclusion: one sentence naming the root cause; if the log is insufficient, write \"insufficient log\" and say what is missing.\nKey lines: at most 2, each quoting a short log fragment (<=20 chars).\nFix: at most 2 actionable steps, one sentence each.";

const SYSTEM_PROMPT_EN_BALANCED: &str = "You are the log-diagnostics assistant of a Minecraft launcher. Answer using ONLY the redacted log the user provides. Never invent or infer facts absent from the log, and never restate the log verbatim. Reply in English, 300-400 words, in exactly these three sections, with no preamble or closing:\nConclusion: up to 2 sentences naming the root cause; if insufficient, say so and what is missing.\nKey lines: at most 3, each quoting a short log fragment (<=20 chars).\nFix: at most 3 actionable steps, one sentence each; name concrete config keys or commands when relevant.";

const SYSTEM_PROMPT_EN_DETAILED: &str = "You are the log-diagnostics assistant of a Minecraft launcher. Answer using ONLY the redacted log the user provides. Never invent or infer facts absent from the log, and never restate the log verbatim. Reply in English, 600-800 words, in exactly these three sections, with no preamble or closing:\nConclusion: explain the root cause and trigger conditions.\nKey lines: at most 5, each quoting a short log fragment (<=20 chars).\nFix: prioritized, concrete steps (versions, loader, config files, commands); when information is missing, state how to obtain it.";

fn system_prompt(language: &str, detail: Detail) -> &'static str {
    if language == "en" {
        match detail {
            Detail::Brief => SYSTEM_PROMPT_EN_BRIEF,
            Detail::Balanced => SYSTEM_PROMPT_EN_BALANCED,
            Detail::Detailed => SYSTEM_PROMPT_EN_DETAILED,
        }
    } else {
        match detail {
            Detail::Brief => SYSTEM_PROMPT_ZH_BRIEF,
            Detail::Balanced => SYSTEM_PROMPT_ZH_BALANCED,
            Detail::Detailed => SYSTEM_PROMPT_ZH_DETAILED,
        }
    }
}

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
    /// 本次分析绑定的运行 id（前端据此隔离结果）。
    pub run_id: Option<String>,
    /// 本次实际发送给模型的脱敏日志行数。
    pub sent_lines: usize,
    /// 被筛选省略的行数（激进模式；标准模式为 0）。
    pub omitted_lines: usize,
    /// 日志来源：`captured` / `disk-fresh` / `disk-unbound`。
    pub source: String,
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

/// 一次分析的输入（已脱敏、已按提交模式筛选）。
struct AnalysisInput {
    log_text: String,
    context: String,
    sent_lines: usize,
    omitted_lines: usize,
    source: &'static str,
}

/// 从原始日志中按提交模式选取要发送的文本。
fn select_log_text(raw: &str, mode: LogMode) -> (String, usize) {
    match mode {
        LogMode::Standard => (
            game_log::tail_chars(raw, ANALYSIS_STANDARD_WINDOW_CHARS).to_string(),
            0,
        ),
        LogMode::Aggressive => {
            let total = raw.lines().count();
            let kept: Vec<&str> = raw
                .lines()
                .filter(|line| {
                    line.contains("ERROR") || line.contains("WARN") || line.contains("FATAL")
                })
                .collect();
            let omitted = total.saturating_sub(kept.len());
            let joined = kept.join("\n");
            let text = game_log::tail_chars(&joined, ANALYSIS_AGGRESSIVE_MAX_CHARS).to_string();
            (text, omitted)
        }
    }
}

/// 实例上下文（名称 / 版本 / 加载器）。
async fn instance_context(state: &AppState, key: &str) -> Option<(String, String, String)> {
    let core = state.core.read().await;
    let mut service = core.instances.lock().ok()?;
    let detail = service.get(key).ok()?;
    let loader = detail
        .summary
        .loader
        .as_ref()
        .map(|loader| loader.kind.clone())
        .unwrap_or_else(|| "vanilla".to_string());
    Some((
        detail.summary.display_name.clone(),
        detail.summary.game_version.clone(),
        loader,
    ))
}

/// 上下文一行（本地化）+ 退出码/来源说明。
fn build_context(
    language: &str,
    instance: Option<(String, String, String)>,
    source: LogSourceKind,
    exit_code: Option<i32>,
    exit_type: Option<&str>,
) -> String {
    let zh = language != "en";
    let mut context = match (&instance, zh) {
        (Some((name, version, loader)), true) => {
            format!("实例「{name}」：Minecraft {version}，加载器 {loader}")
        }
        (Some((name, version, loader)), false) => {
            format!("Instance \"{name}\": Minecraft {version}, loader {loader}")
        }
        (None, true) => "共享游戏目录（未指定实例）".to_string(),
        (None, false) => "Shared game directory (no instance)".to_string(),
    };
    if let Some(code) = exit_code {
        if zh {
            context.push_str(&format!("；退出码 {code}"));
            if let Some(kind) = exit_type {
                context.push_str(&format!("（{kind}）"));
            }
        } else {
            context.push_str(&format!("; exit code {code}"));
            if let Some(kind) = exit_type {
                context.push_str(&format!(" ({kind})"));
            }
        }
    }
    let source_label = match (source, zh) {
        (LogSourceKind::Captured, true) => "启动器捕获的本轮输出",
        (LogSourceKind::DiskFresh, true) => "本轮 latest.log",
        (LogSourceKind::DiskUnbound, true) => "磁盘 latest.log（未绑定运行）",
        (LogSourceKind::Captured, false) => "launcher-captured output (this run)",
        (LogSourceKind::DiskFresh, false) => "latest.log (this run)",
        (LogSourceKind::DiskUnbound, false) => "latest.log (run unbound)",
    };
    if zh {
        context.push_str(&format!("；日志来源：{source_label}"));
    } else {
        context.push_str(&format!("; log source: {source_label}"));
    }
    context
}

async fn gather_analysis_input(
    state: &AppState,
    key: Option<&str>,
    mode: LogMode,
    language: &str,
    exit_code: Option<i32>,
    exit_type: Option<&str>,
) -> rmccc_error::Result<AnalysisInput> {
    let window = match mode {
        LogMode::Standard => ANALYSIS_STANDARD_WINDOW_CHARS,
        LogMode::Aggressive => ANALYSIS_AGGRESSIVE_SCAN_CHARS,
    };
    let resolved = game_log::resolve_fresh_log(state, key, window).await?;
    let (selected, omitted_lines) = select_log_text(&resolved.text, mode);
    if selected.trim().is_empty() {
        return Err(RmcccError::Validation(if mode == LogMode::Aggressive {
            "日志中没有 ERROR / WARNING 记录；可切换到“标准”模式提交完整日志".into()
        } else {
            "本次运行尚未产生可分析的日志内容".into()
        }));
    }
    // 脱敏（home → ~、凭据遮蔽）在发送前执行。
    let log_text = redact::redact_text(&selected, redact::user_home().as_deref());
    let sent_lines = log_text.lines().count();
    let instance = match key {
        Some(key) => instance_context(state, key).await,
        None => None,
    };
    let context = build_context(language, instance, resolved.kind, exit_code, exit_type);
    Ok(AnalysisInput {
        log_text,
        context,
        sent_lines,
        omitted_lines,
        source: resolved.kind.as_str(),
    })
}

fn build_user_message(language: &str, input: &AnalysisInput, mode: LogMode) -> String {
    let zh = language != "en";
    let mode_line = match (mode, zh) {
        (LogMode::Standard, true) => "提交模式：标准（完整日志尾部）".to_string(),
        (LogMode::Aggressive, true) => {
            format!(
                "提交模式：激进（仅 ERROR/WARNING 行，已省略 {} 行）",
                input.omitted_lines
            )
        }
        (LogMode::Standard, false) => "Mode: standard (full log tail)".to_string(),
        (LogMode::Aggressive, false) => format!(
            "Mode: aggressive (ERROR/WARNING lines only; {} lines omitted)",
            input.omitted_lines
        ),
    };
    let divider = if zh {
        "----- 日志（脱敏）-----"
    } else {
        "----- Log (redacted) -----"
    };
    format!(
        "{}\n{}\n{}\n{}",
        input.context, mode_line, divider, input.log_text
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(state))]
pub async fn ai_analyze_log(
    state: tauri::State<'_, AppState>,
    key: Option<String>,
    run_id: Option<String>,
    detail: Option<String>,
    log_mode: Option<String>,
    language: Option<String>,
    exit_code: Option<i32>,
    exit_type: Option<String>,
) -> Result<AiAnalysisHandle, IpcError> {
    let credential = load_credential(&state).await?;
    let language = if language.as_deref() == Some("en") {
        "en"
    } else {
        "zh-CN"
    };
    let mode = LogMode::parse(log_mode.as_deref());
    let detail = Detail::parse(detail.as_deref());
    let input = gather_analysis_input(
        &state,
        key.as_deref(),
        mode,
        language,
        exit_code,
        exit_type.as_deref(),
    )
    .await?;

    let analysis_id = uuid::Uuid::new_v4().simple().to_string();
    let cancel = CancelToken::new();
    {
        let mut analyses = state
            .ai_analyses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if analyses.len() >= MAX_CONCURRENT_ANALYSES {
            return Err(IpcError {
                code: ErrorCode::AlreadyRunning,
                message: "too many concurrent AI analyses".into(),
                retryable: false,
            });
        }
        analyses.insert(
            analysis_id.clone(),
            AnalysisRecord {
                cancel: cancel.clone(),
                profile_id: key.clone(),
            },
        );
    }

    let system = system_prompt(language, detail);
    let user = build_user_message(language, &input, mode);
    let handle = AiAnalysisHandle {
        analysis_id: analysis_id.clone(),
        run_id: run_id.clone(),
        sent_lines: input.sent_lines,
        omitted_lines: input.omitted_lines,
        source: input.source.to_string(),
    };

    let app = state.app_handle.clone();
    let analyses = state.ai_analyses.clone();
    tauri::async_runtime::spawn(async move {
        run_analysis(
            app,
            analyses,
            analysis_id,
            credential,
            system,
            user,
            detail.max_tokens(),
            cancel,
        )
        .await;
    });

    Ok(handle)
}

#[allow(clippy::too_many_arguments)]
async fn run_analysis(
    app: tauri::AppHandle,
    analyses: Arc<std::sync::Mutex<HashMap<String, AnalysisRecord>>>,
    analysis_id: String,
    credential: ServiceCredential,
    system: &'static str,
    user: String,
    max_tokens: u32,
    cancel: CancelToken,
) {
    use tauri::Emitter;

    let provider = match OpenAiCompatibleProvider::new(
        &credential,
        LlmOptions {
            timeout: Duration::from_secs(ANALYSIS_TIMEOUT_SECS),
            first_byte_timeout: Duration::from_secs(ANALYSIS_FIRST_BYTE_TIMEOUT_SECS),
            ..Default::default()
        },
    ) {
        Ok(provider) => provider,
        Err(error) => {
            emit_ai_error(&app, &analysis_id, &error.to_string());
            remove_analysis(&analyses, &analysis_id);
            return;
        }
    };
    let mut request = ChatRequest::new(vec![ChatMessage::system(system), ChatMessage::user(user)]);
    request.temperature = Some(ANALYSIS_TEMPERATURE);
    request.max_tokens = Some(max_tokens);

    let mut stream = match provider.stream_chat(request, cancel) {
        Ok(stream) => stream,
        Err(error) => {
            emit_ai_error(&app, &analysis_id, &error.to_string());
            remove_analysis(&analyses, &analysis_id);
            return;
        }
    };

    // 终止保证：三条终止事件分支都会向 UI 回报，绝不静默结束。
    let mut terminated = false;
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
                emit_ai_done(&app, &analysis_id, "complete");
                terminated = true;
                break;
            }
            LlmEvent::Cancelled => {
                emit_ai_done(&app, &analysis_id, "cancelled");
                terminated = true;
                break;
            }
            LlmEvent::Error(error) => {
                emit_ai_error(&app, &analysis_id, &error.to_string());
                terminated = true;
                break;
            }
        }
    }
    if !terminated {
        // 流在没有任何终止事件的情况下关闭（理论不应发生）：兜底报错，
        // 保证前端不会停在"分析中"。
        emit_ai_error(&app, &analysis_id, "AI 流意外结束，请重试");
    }
    remove_analysis(&analyses, &analysis_id);
}

fn emit_ai_done(app: &tauri::AppHandle, analysis_id: &str, reason: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        crate::events::AI_DONE,
        &crate::events::AiDonePayload {
            analysis_id: analysis_id.to_string(),
            reason: reason.to_string(),
        },
    );
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

fn remove_analysis(
    analyses: &Arc<std::sync::Mutex<HashMap<String, AnalysisRecord>>>,
    analysis_id: &str,
) {
    let mut analyses = analyses
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    analyses.remove(analysis_id);
}

/// 取消某实例名下所有在途分析（新运行开始时调用，旧运行的分析即失效）。
pub(crate) fn cancel_analyses_for(
    analyses: &Arc<std::sync::Mutex<HashMap<String, AnalysisRecord>>>,
    profile_id: &str,
) {
    let mut analyses = analyses
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let ids: Vec<String> = analyses
        .iter()
        .filter(|(_, record)| record.profile_id.as_deref() == Some(profile_id))
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(record) = analyses.remove(&id) {
            record.cancel.cancel();
        }
    }
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn ai_analyze_cancel(
    state: tauri::State<'_, AppState>,
    analysis_id: String,
) -> Result<bool, IpcError> {
    let mut analyses = state
        .ai_analyses
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(analyses.remove(&analysis_id).is_some_and(|record| {
        record.cancel.cancel();
        true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn aggressive_mode_keeps_only_error_and_warning_lines() {
        let raw = "[INFO] loading\n[WARN] bad config\n[INFO] fine\n[ERROR] crash\n[FATAL] dead";
        let (text, omitted) = select_log_text(raw, LogMode::Aggressive);
        assert!(text.contains("[WARN] bad config"));
        assert!(text.contains("[ERROR] crash"));
        assert!(text.contains("[FATAL] dead"));
        assert!(!text.contains("[INFO] loading"));
        assert_eq!(omitted, 2);
    }

    #[test]
    fn standard_mode_keeps_the_full_tail() {
        let raw = "[INFO] loading\n[WARN] bad config\n[ERROR] crash";
        let (text, omitted) = select_log_text(raw, LogMode::Standard);
        assert_eq!(text, raw);
        assert_eq!(omitted, 0);
    }

    #[test]
    fn modes_and_details_parse_with_defaults() {
        assert_eq!(LogMode::parse(Some("aggressive")), LogMode::Aggressive);
        assert_eq!(LogMode::parse(None), LogMode::Standard);
        assert_eq!(Detail::parse(Some("brief")), Detail::Brief);
        assert_eq!(Detail::parse(Some("detailed")), Detail::Detailed);
        assert_eq!(Detail::parse(Some("x")), Detail::Balanced);
        assert!(Detail::Detailed.max_tokens() > Detail::Brief.max_tokens());
    }
}
