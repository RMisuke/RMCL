//! 轻量 AI 服务（ADR-013）：启动器内部的 OpenAI 兼容端口。
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`credentials`] | AI 服务凭据的加密存取（AES-256-GCM 文件 vault，ADR-009） |
//! | [`llm`] | OpenAI 兼容 chat completions 适配器（流式/重试/取消/限流）+ 模型探测 |
//! | [`redact`] | 脱敏规则（凭据串 / 查询参数 / home 路径 / 邮箱），发送与落盘前执行 |
//!
//! 这是「轻干预 · 轻 AI」的内核：**没有编排循环、没有工具注册表、没有会话
//! 状态机**——只有"发消息 → 收流式回复"。AI 编排在外部工具（经 MCP，后续切片），
//! 启动器内置功能与插件只做单次调用。
//!
//! # 安全
//!
//! - API Key 只在 Rust 侧解密后使用；公开类型 [`credentials::CredentialSummary`]
//!   不含 `api_key`，`Debug` 打码。
//! - 远程端点必须 HTTPS；明文只允许 loopback（本地模型，ADR-009 决策 5）。
//! - 向模型发送任何用户数据（日志分析等）前，调用方必须先经 [`redact`] 脱敏
//!   并向用户说明数据范围。

pub mod credentials;
pub mod llm;
pub mod redact;

pub use credentials::{
    validate_endpoint, CredentialStore, CredentialSummary, FileCredentialStore, ServiceCredential,
};
pub use llm::{
    probe_models, CancelToken, ChatMessage, ChatOutcome, ChatRequest, ChatRole, LlmError, LlmEvent,
    LlmOptions, LlmProvider, LlmStream, ModelCapabilities, OpenAiCompatibleProvider, RetryPolicy,
};
pub use redact::{redact_json, redact_text, user_home};
