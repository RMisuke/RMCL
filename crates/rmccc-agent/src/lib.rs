//! Agent platform primitives (Phase 6, ADR-009/ADR-010).
//!
//! This crate deliberately contains **no execution loop**. It provides the pieces
//! that a future `AgentRuntime` (Phase 6.5) must schedule through:
//!
//! | 模块 | 职责 | ADR |
//! |------|------|-----|
//! | [`credentials`] | 服务凭据的加密存取（AES-256-GCM 文件 vault） | ADR-009 |
//! | [`llm`] | 统一的 LLM Provider adapter（能力/流式/超时/重试/限流/取消） | 指南 §10.4 |
//! | [`tools`] | 工具描述符、JSON Schema 校验、只读工具注册表 | 指南 §10.2 |
//! | [`mods`] | 只读兼容性扫描与 `ModInstallPlan`（消费 [`rmccc_mods::ModDependency`]） | 指南 §10.6 |
//! | [`audit`] | 工具调用审计（JSONL，写盘前脱敏） | ADR-010 |
//! | [`session`] | 会话事件流 / 任务快照 / 保留期裁剪（`agent/sessions`、`agent/tasks`） | ADR-010 |
//! | [`redact`] | 脱敏规则（凭据串 / 查询参数 / home 路径 / 邮箱） | ADR-010 |
//!
//! # 安全边界
//!
//! - 凭据只能由 Rust 侧读取；公开的 [`credentials::CredentialSummary`] 不含 `api_key`。
//! - [`tools::ToolRegistry::invoke`] 在 Phase 6.6 只放行 `read_only` 工具；
//!   写操作必须等 Phase 6.5 的审批链路，不在这里开洞。
//! - 审计与日志写盘前经过 [`redact`]，磁盘上不应出现凭据原文。
//! - 会话事件只记白名单字段：提示词/模型输出只有哈希与长度（[`session`]）。

pub mod audit;
pub mod credentials;
pub mod llm;
pub mod mods;
pub mod redact;
pub mod runtime;
pub mod session;
pub mod tools;

pub use audit::{AuditLog, AuditOutcome, AuditRecord};
pub use credentials::{
    validate_endpoint, CredentialStore, CredentialSummary, FileCredentialStore, ServiceCredential,
};
pub use llm::{
    CancelToken, ChatMessage, ChatOutcome, ChatRequest, ChatRole, LlmError, LlmEvent, LlmOptions,
    LlmProvider, LlmStream, ModelCapabilities, OpenAiCompatibleProvider, RetryPolicy,
};
pub use mods::{
    full_registry, read_only_registry, scan_instance_mods, AgentRoots, AppliedMod,
    AppliedPlanReport, DependencyReport, FailedMod, InstalledMod, MetadataSource, ModConflict,
    ModInstallPlan, PlanApplier, PlannedDependency,
};
pub use runtime::{
    AgentService, AgentServiceConfig, ApprovalBroker, Resolution, ToolCallRequest, TurnOutput,
    TurnStatus,
};
pub use session::{
    is_expired, validate_id, PurgeReport, SessionEvent, SessionEventKind, SessionStore,
    SessionSummary, TaskSnapshot, TaskStatus, DEFAULT_RETENTION_DAYS,
};
pub use tools::{
    AgentTool, AgentToolDescriptor, ConfirmationPolicy, RiskLevel, ToolRegistry, ToolResult,
};
