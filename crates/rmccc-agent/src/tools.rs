//! Agent 工具契约与只读注册表（Phase 6.6，指南 §10.2）。
//!
//! # 设计要点
//!
//! - **描述符先于实现**：模型只能看到 [`AgentToolDescriptor`]（id/描述/输入 Schema/风险等级/
//!   超时/确认策略），不能看到 Rust 函数。
//! - **参数先校验**：`invoke` 在任何副作用之前按 `input_schema` 校验；未知字段默认拒绝
//!   （`additionalProperties: false`），避免模型夹带参数。
//! - **风险门禁**：Phase 6.6 只注册 `read_only` 工具；`invoke` 对更高风险等级直接拒绝。
//!   写操作要等 Phase 6.5 的审批链路（ADR-010、指南 §10.3）。
//! - **可审计**：注册表可持有 [`crate::audit::AuditLog`]，每次调用（成功或失败）写一条记录。

use crate::audit::{AuditLog, AuditOutcome, AuditRecord};
use rmccc_error::{Result, RmcccError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// 风险分级（指南 §10.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    ReversibleWrite,
    Destructive,
    ExternalSideEffect,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskLevel::ReadOnly => "read_only",
            RiskLevel::ReversibleWrite => "reversible_write",
            RiskLevel::Destructive => "destructive",
            RiskLevel::ExternalSideEffect => "external_side_effect",
        }
    }
}

/// 是否需要用户确认（策略字段，执行在 Phase 6.5 的 Approval Broker）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationPolicy {
    Never,
    PerPolicy,
    Always,
}

/// 工具描述符（指南 §10.2）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolDescriptor {
    pub id: String,
    /// 内置工具为 `None`；插件注册的工具填插件 id。
    pub plugin_id: Option<String>,
    pub name: String,
    pub description: String,
    /// 最小 JSON Schema 子集（object/array/string/integer/number/boolean/null）。
    pub input_schema: Value,
    pub output_schema: Value,
    pub required_capabilities: Vec<String>,
    pub risk_level: RiskLevel,
    pub timeout_ms: u64,
    pub confirmation_policy: ConfirmationPolicy,
}

/// 规范化结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool_id: String,
    pub ok: bool,
    pub output: Value,
}

impl ToolResult {
    pub fn ok(tool_id: impl Into<String>, output: Value) -> Self {
        Self {
            tool_id: tool_id.into(),
            ok: true,
            output,
        }
    }
}

/// 工具实现接口。
///
/// **不是**"模型可调用的任意函数"：执行由 [`ToolRegistry::invoke`] 调度，
/// 参数已校验、风险已判定、审计已就位。
pub trait AgentTool: Send + Sync {
    fn descriptor(&self) -> &AgentToolDescriptor;
    fn execute(&self, input: &Value) -> Result<Value>;

    /// 写工具的两阶段协议：先产出"给用户审阅的载荷"（dry-run 计划），
    /// 审批通过后宿主再经 [`ToolRegistry::invoke_approved`] 调 [`Self::execute`]。
    ///
    /// 只读工具默认 `None`（不需要独立审阅载荷）。实现方必须保证
    /// `execute` 自己重新校验现状（审批与执行之间状态可能变化——
    /// "用户确认 + Core 再校验"，指南 §10.6）。
    fn approval_payload(&self, _input: &Value) -> Result<Option<Value>> {
        Ok(None)
    }
}

/// 只读工具注册表。
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn AgentTool>>,
    audit: Option<Arc<AuditLog>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
            audit: None,
        }
    }

    /// 挂审计日志；之后每次 `invoke` 都会追加一条记录（包括拒绝）。
    pub fn with_audit(mut self, audit: Arc<AuditLog>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 注册工具。重复 id 直接拒绝，避免后注册覆盖先注册（插件冲突的常见来源）。
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) -> Result<()> {
        let descriptor = tool.descriptor();
        validate_descriptor(descriptor)?;
        if self.tools.contains_key(&descriptor.id) {
            return Err(RmcccError::Validation(format!(
                "duplicate agent tool id: {}",
                descriptor.id
            )));
        }
        self.tools.insert(descriptor.id.clone(), tool);
        Ok(())
    }

    pub fn descriptors(&self) -> Vec<AgentToolDescriptor> {
        self.tools
            .values()
            .map(|tool| tool.descriptor().clone())
            .collect()
    }

    pub fn descriptor(&self, tool_id: &str) -> Option<AgentToolDescriptor> {
        self.tools
            .get(tool_id)
            .map(|tool| tool.descriptor().clone())
    }

    /// 取工具的两阶段审阅载荷（写工具在审批前先给用户看 dry-run 计划）。
    pub fn approval_payload(&self, tool_id: &str, input: &Value) -> Result<Option<Value>> {
        let tool = self
            .tools
            .get(tool_id)
            .ok_or_else(|| RmcccError::Validation(format!("unknown agent tool: {tool_id}")))?;
        tool.approval_payload(input)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// 调用只读工具：校验 → 执行 → 审计。
    pub fn invoke(&self, tool_id: &str, input: &Value) -> Result<ToolResult> {
        self.invoke_in_session("local", tool_id, input)
    }

    /// 审批后的调用——**写工具的唯一通道**。
    ///
    /// 调用方（执行循环）负责取得已批准的任务快照；这里校验快照与工具
    /// 的对应关系（状态 = approved、kind 指向同一工具、会话一致），
    /// 防止拿 A 工具的批准执行 B 工具，或重放已决断的任务。
    /// 成功/失败后由调用方把任务推进到 applied / failed。
    pub fn invoke_approved(
        &self,
        session_id: &str,
        task: &crate::session::TaskSnapshot,
        tool_id: &str,
        input: &Value,
    ) -> Result<ToolResult> {
        use crate::session::TaskStatus;
        let started = Instant::now();

        if task.status != TaskStatus::Approved {
            let error = RmcccError::Validation(format!(
                "task {} is {:?}, not approved",
                task.task_id, task.status
            ));
            let _ = self.audit_failure(
                session_id,
                tool_id,
                RiskLevel::ReversibleWrite,
                input,
                started,
                &error,
            );
            return Err(error);
        }
        if task.kind != format!("tool_call:{tool_id}") {
            let error = RmcccError::Validation(format!(
                "task {} approves '{}' but invoke was for '{tool_id}'",
                task.task_id, task.kind
            ));
            let _ = self.audit_failure(
                session_id,
                tool_id,
                RiskLevel::ReversibleWrite,
                input,
                started,
                &error,
            );
            return Err(error);
        }
        if task.session_id.as_deref() != Some(session_id) {
            let error =
                RmcccError::Validation(format!("task {} belongs to another session", task.task_id));
            let _ = self.audit_failure(
                session_id,
                tool_id,
                RiskLevel::ReversibleWrite,
                input,
                started,
                &error,
            );
            return Err(error);
        }

        let Some(tool) = self.tools.get(tool_id) else {
            let error = RmcccError::Validation(format!("unknown agent tool: {tool_id}"));
            let _ = self.audit_failure(
                session_id,
                tool_id,
                RiskLevel::ReversibleWrite,
                input,
                started,
                &error,
            );
            return Err(error);
        };
        let descriptor = tool.descriptor();
        if let Err(error) = validate_json_schema(&descriptor.input_schema, input) {
            let _ = self.audit_failure(
                session_id,
                tool_id,
                descriptor.risk_level,
                input,
                started,
                &error,
            );
            return Err(error);
        }

        // 成功路径 fail-closed：审计写不进去就不返回"已执行"。
        match tool.execute(input) {
            Ok(output) => {
                let result = ToolResult::ok(descriptor.id.clone(), output);
                self.audit_success(session_id, descriptor, input, started)?;
                Ok(result)
            }
            Err(error) => {
                let _ = self.audit_failure(
                    session_id,
                    tool_id,
                    descriptor.risk_level,
                    input,
                    started,
                    &error,
                );
                Err(error)
            }
        }
    }

    /// 指定会话的调用（审计用；**只放行 read_only 工具**）。
    pub fn invoke_in_session(
        &self,
        session_id: &str,
        tool_id: &str,
        input: &Value,
    ) -> Result<ToolResult> {
        let started = Instant::now();
        let Some(tool) = self.tools.get(tool_id) else {
            let error = RmcccError::Validation(format!("unknown agent tool: {tool_id}"));
            // 拒绝路径：审计失败不改变"调用被拒绝"这一结果。
            let _ = self.audit_failure(
                session_id,
                tool_id,
                RiskLevel::ReadOnly,
                input,
                started,
                &error,
            );
            return Err(error);
        };
        let descriptor = tool.descriptor();

        if descriptor.risk_level != RiskLevel::ReadOnly {
            let error = RmcccError::Validation(format!(
                "agent tool '{}' is {}; only read_only tools are enabled in this phase",
                descriptor.id,
                descriptor.risk_level.as_str()
            ));
            // 拒绝路径：审计失败不改变"调用被拒绝"这一结果。
            let _ = self.audit_failure(
                session_id,
                tool_id,
                descriptor.risk_level,
                input,
                started,
                &error,
            );
            return Err(error);
        }

        if let Err(error) = validate_json_schema(&descriptor.input_schema, input) {
            // 拒绝路径：审计失败不改变"调用被拒绝"这一结果。
            let _ = self.audit_failure(
                session_id,
                tool_id,
                descriptor.risk_level,
                input,
                started,
                &error,
            );
            return Err(error);
        }

        match tool.execute(input) {
            Ok(output) => {
                let result = ToolResult::ok(descriptor.id.clone(), output);
                // 审计失败不能让调用"看起来成功又查不到"（ADR-010 决策 3）：
                // 成功路径 fail-closed，把审计错误返回给调用方。
                self.audit_success(session_id, descriptor, input, started)?;
                Ok(result)
            }
            Err(error) => {
                // 失败路径：调用本身已经失败，审计写失败时保留原始工具错误
                //（日志里留审计错误），不让调用方看到更晦涩的第二层错误。
                if let Err(audit_error) = self.audit_failure(
                    session_id,
                    tool_id,
                    descriptor.risk_level,
                    input,
                    started,
                    &error,
                ) {
                    tracing::error!(
                        tool = %tool_id,
                        error = %audit_error,
                        "failed to write agent audit record for a failed tool call"
                    );
                }
                Err(error)
            }
        }
    }

    fn audit_success(
        &self,
        session_id: &str,
        descriptor: &AgentToolDescriptor,
        input: &Value,
        started: Instant,
    ) -> Result<()> {
        let Some(audit) = &self.audit else {
            return Ok(());
        };
        let record = AuditRecord::new(
            session_id,
            descriptor.id.clone(),
            descriptor.risk_level,
            input,
            AuditOutcome::Ok,
            started.elapsed(),
            None,
        );
        audit.append(&record)
    }

    fn audit_failure(
        &self,
        session_id: &str,
        tool_id: &str,
        risk_level: RiskLevel,
        input: &Value,
        started: Instant,
        error: &RmcccError,
    ) -> Result<()> {
        let Some(audit) = &self.audit else {
            return Ok(());
        };
        let record = AuditRecord::new(
            session_id,
            tool_id.to_string(),
            risk_level,
            input,
            if matches!(error, RmcccError::Validation(_)) {
                AuditOutcome::Denied
            } else {
                AuditOutcome::Error
            },
            started.elapsed(),
            Some(&error.to_string()),
        );
        audit.append(&record)
    }
}

fn validate_descriptor(descriptor: &AgentToolDescriptor) -> Result<()> {
    if descriptor.id.is_empty()
        || !descriptor
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(RmcccError::Validation(format!(
            "agent tool id '{}' must match [A-Za-z0-9._-]+",
            descriptor.id
        )));
    }
    if descriptor.input_schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(RmcccError::Validation(format!(
            "agent tool '{}' input_schema must be an object schema",
            descriptor.id
        )));
    }
    if descriptor.timeout_ms == 0 || descriptor.timeout_ms > 60_000 {
        return Err(RmcccError::Validation(format!(
            "agent tool '{}' timeout_ms must be 1..=60000",
            descriptor.id
        )));
    }
    Ok(())
}

/// 最小 JSON Schema 校验器（无第三方依赖）。
///
/// 支持：`type`（含 type 数组）、`required`、`properties`、`additionalProperties: false`、
/// `items`、`enum`、`minimum`/`maximum`、`minLength`/`maxLength`。
/// 不支持 `$ref`、`oneOf`/`anyOf` 等——工具 Schema 应保持扁平（指南 §10.2 的窄接口）。
pub fn validate_json_schema(schema: &Value, value: &Value) -> Result<()> {
    validate_at(schema, value, "$")
}

fn validate_at(schema: &Value, value: &Value, path: &str) -> Result<()> {
    let Some(object) = schema.as_object() else {
        // 没有 schema 约束（空对象/true）视为通过。
        return Ok(());
    };

    if let Some(type_spec) = object.get("type") {
        let matches = match type_spec {
            Value::String(name) => type_matches(name, value),
            Value::Array(names) => names
                .iter()
                .filter_map(Value::as_str)
                .any(|name| type_matches(name, value)),
            _ => true,
        };
        if !matches {
            return Err(invalid(
                path,
                format!(
                    "expected type {}, got {}",
                    describe_type(type_spec),
                    kind_of(value)
                ),
            ));
        }
    }

    if let Some(enum_values) = object.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(invalid(path, "value is not one of the allowed enum values"));
        }
    }

    match value {
        Value::Object(map) => {
            if let Some(required) = object.get("required").and_then(Value::as_array) {
                for key in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(key) {
                        return Err(invalid(path, format!("missing required property '{key}'")));
                    }
                }
            }
            let properties = object.get("properties").and_then(Value::as_object);
            if let Some(properties) = properties {
                for (key, entry) in map {
                    if let Some(property_schema) = properties.get(key) {
                        validate_at(property_schema, entry, &format!("{path}.{key}"))?;
                    } else if object.get("additionalProperties") == Some(&Value::Bool(false)) {
                        return Err(invalid(path, format!("unknown property '{key}'")));
                    }
                }
            } else if object.get("additionalProperties") == Some(&Value::Bool(false))
                && !map.is_empty()
            {
                return Err(invalid(path, "object has no declared properties"));
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = object.get("items") {
                for (index, item) in items.iter().enumerate() {
                    validate_at(item_schema, item, &format!("{path}[{index}]"))?;
                }
            }
        }
        Value::String(text) => {
            if let Some(min) = object.get("minLength").and_then(Value::as_u64) {
                if (text.chars().count() as u64) < min {
                    return Err(invalid(path, format!("string shorter than {min}")));
                }
            }
            if let Some(max) = object.get("maxLength").and_then(Value::as_u64) {
                if (text.chars().count() as u64) > max {
                    return Err(invalid(path, format!("string longer than {max}")));
                }
            }
        }
        Value::Number(number) => {
            let numeric = number.as_f64().unwrap_or(0.0);
            if let Some(min) = object.get("minimum").and_then(Value::as_f64) {
                if numeric < min {
                    return Err(invalid(path, format!("number below minimum {min}")));
                }
            }
            if let Some(max) = object.get("maximum").and_then(Value::as_f64) {
                if numeric > max {
                    return Err(invalid(path, format!("number above maximum {max}")));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn invalid(path: &str, message: impl Into<String>) -> RmcccError {
    RmcccError::Validation(format!(
        "input schema violation at {path}: {}",
        message.into()
    ))
}

fn type_matches(name: &str, value: &Value) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn describe_type(type_spec: &Value) -> String {
    match type_spec {
        Value::String(name) => name.clone(),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ => "unknown".into(),
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 便捷构造：只读工具的输入 Schema。
pub(crate) fn object_schema(properties: Value, required: &[&str]) -> Value {
    let mut map = Map::new();
    map.insert("type".into(), Value::String("object".into()));
    map.insert("properties".into(), properties);
    map.insert(
        "required".into(),
        Value::Array(
            required
                .iter()
                .map(|r| Value::String((*r).into()))
                .collect(),
        ),
    );
    map.insert("additionalProperties".into(), Value::Bool(false));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    struct EchoTool {
        descriptor: AgentToolDescriptor,
    }

    impl EchoTool {
        fn new(risk: RiskLevel) -> Self {
            Self {
                descriptor: AgentToolDescriptor {
                    id: "demo.echo".into(),
                    plugin_id: None,
                    name: "echo".into(),
                    description: "echoes its input".into(),
                    input_schema: object_schema(json!({ "text": { "type": "string" } }), &["text"]),
                    output_schema: json!({ "type": "object" }),
                    required_capabilities: vec![],
                    risk_level: risk,
                    timeout_ms: 1000,
                    confirmation_policy: ConfirmationPolicy::Never,
                },
            }
        }
    }

    impl AgentTool for EchoTool {
        fn descriptor(&self) -> &AgentToolDescriptor {
            &self.descriptor
        }

        fn execute(&self, input: &Value) -> Result<Value> {
            Ok(json!({ "echo": input["text"] }))
        }
    }

    #[test]
    fn registers_and_invokes_read_only_tool() {
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(EchoTool::new(RiskLevel::ReadOnly)))
            .expect("register");
        assert_eq!(registry.len(), 1);
        let result = registry
            .invoke("demo.echo", &json!({ "text": "hi" }))
            .expect("invoke");
        assert!(result.ok);
        assert_eq!(result.output["echo"], "hi");
    }

    #[test]
    fn rejects_write_tools_even_if_registered() {
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(EchoTool::new(RiskLevel::Destructive)))
            .expect("register");
        let error = registry
            .invoke("demo.echo", &json!({ "text": "hi" }))
            .expect_err("must reject");
        assert!(error.to_string().contains("only read_only"));
    }

    #[test]
    fn rejects_unknown_tool_and_duplicate_ids() {
        let mut registry = ToolRegistry::new();
        assert!(registry.invoke("nope", &json!({})).is_err());
        registry
            .register(Arc::new(EchoTool::new(RiskLevel::ReadOnly)))
            .expect("register");
        assert!(registry
            .register(Arc::new(EchoTool::new(RiskLevel::ReadOnly)))
            .is_err());
    }

    #[test]
    fn schema_validation_covers_required_types_and_unknown_fields() {
        let schema = object_schema(
            json!({
                "dir": { "type": "string", "minLength": 1 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 10 },
                "mode": { "type": "string", "enum": ["a", "b"] }
            }),
            &["dir"],
        );
        assert!(validate_json_schema(&schema, &json!({ "dir": "/tmp" })).is_ok());
        assert!(validate_json_schema(&schema, &json!({})).is_err());
        assert!(validate_json_schema(&schema, &json!({ "dir": "/tmp", "limit": 0 })).is_err());
        assert!(validate_json_schema(&schema, &json!({ "dir": "/tmp", "limit": 11 })).is_err());
        assert!(validate_json_schema(&schema, &json!({ "dir": "/tmp", "mode": "c" })).is_err());
        assert!(validate_json_schema(&schema, &json!({ "dir": "/tmp", "extra": 1 })).is_err());
        assert!(validate_json_schema(&schema, &json!({ "dir": "" })).is_err());
    }

    #[test]
    fn schema_validation_handles_arrays_and_type_unions() {
        let schema = json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": { "type": "array", "items": { "type": "string" } },
                "optional": { "type": ["string", "null"] }
            }
        });
        assert!(validate_json_schema(&schema, &json!({ "items": ["a", "b"] })).is_ok());
        assert!(validate_json_schema(&schema, &json!({ "items": ["a", 1] })).is_err());
        assert!(validate_json_schema(&schema, &json!({ "items": [], "optional": null })).is_ok());
    }

    #[test]
    fn descriptor_validation_rejects_bad_ids_and_timeouts() {
        let mut registry = ToolRegistry::new();
        let mut tool = EchoTool::new(RiskLevel::ReadOnly);
        tool.descriptor.id = "bad id".into();
        assert!(registry.register(Arc::new(tool)).is_err());

        let mut tool = EchoTool::new(RiskLevel::ReadOnly);
        tool.descriptor.timeout_ms = 0;
        assert!(registry.register(Arc::new(tool)).is_err());
    }
}
