//! 实例依赖诊断与操作审计原语（ADR-013 之后的去 Agent 化形态）。
//!
//! Phase 6 的内建 Agent 平台（执行循环 / 工具注册表 / 会话存储）已按
//! ADR-013 退役：AI 编排在外部 AI 工具（经 MCP，后续切片），AI 调用在
//! [`rmccc_llm`]。本 crate 保留并上移仍然有价值的**纯逻辑**：
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`mods`] | Fabric 依赖扫描 / `ModInstallPlan` / 补装执行器 trait（**纯逻辑，不依赖 AI**） |
//! | [`audit`] | 操作审计（JSONL，写盘前脱敏）；MCP/AI 切片起记录工具调用 |
//! | [`confirm`] | 确认快照原语（`TaskSnapshot`，两阶段 proposed→applied，供 MCP 确认流复用） |
//!
//! # 安全边界（沿用 ADR-010 有效部分）
//!
//! - 审计与日志写盘前经 [`rmccc_llm::redact`]，磁盘上不应出现凭据原文。
//! - 目录/文件权限收紧（`PlatformFs::restrict_owner_only*`）语义不变。
//! - `PlanApplier` 实现方必须对每个 jar 做 SHA1 校验，并只写入传入的 `mods_dir`。

pub mod audit;
pub mod confirm;
pub mod mods;

pub use audit::{AuditLog, AuditOutcome, AuditRecord, RiskLevel};
pub use confirm::{TaskSnapshot, TaskStatus};
pub use mods::{
    apply_missing_dependencies, apply_missing_dependencies_async, plan_from_report,
    scan_instance_mods, AppliedMod, AppliedPlanReport, DependencyReport, FailedMod, InstalledMod,
    MetadataSource, ModConflict, ModInstallPlan, PlanApplier, PlannedDependency,
};
