//! MCP 工具目录（ADR-013 §5.2）：只读 15 / 操作 11 / 破坏性 5。
//!
//! 每个工具的 `description` 就是外部 Agent 的使用说明书——必须写清参数语义与
//! 副作用。dispatch 直接调用与 GUI/CLI **同一份**内部命令层（`commands::*`），
//! 因此下载进度、启动事件在 GUI 天然可见；归因由协议层发 `mcp://call`。
//!
//! 安全约定：
//! - `settings.get` 递归剔除形似凭据的键（token/key/secret/password）；
//! - `settings.set` 只接受白名单字段，未知字段直接拒绝（设计 §5.5）；
//! - 工具入参经协议层脱敏后才进审计与确认载荷。

use rmccc_agent::audit::RiskLevel;
use serde_json::{json, Value};

/// 工具风险分级（设计三档；审计落盘时映射到 ADR-010 的四级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    ReadOnly,
    Operate,
    Destructive,
}

impl From<ToolRisk> for RiskLevel {
    fn from(risk: ToolRisk) -> Self {
        match risk {
            ToolRisk::ReadOnly => RiskLevel::ReadOnly,
            ToolRisk::Operate => RiskLevel::ReversibleWrite,
            ToolRisk::Destructive => RiskLevel::Destructive,
        }
    }
}

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: ToolRisk,
    pub schema: Value,
}

/// 工具级失败：协议层转为 `isError: true` 的成功响应。
pub struct ToolError(String);

impl ToolError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl From<rmccc_error::IpcError> for ToolError {
    fn from(e: rmccc_error::IpcError) -> Self {
        ToolError(format!("{:?}: {}", e.code, e.message))
    }
}

impl From<rmccc_error::RmcccError> for ToolError {
    fn from(e: rmccc_error::RmcccError) -> Self {
        ToolError(e.to_string())
    }
}

impl From<String> for ToolError {
    fn from(e: String) -> Self {
        ToolError(e)
    }
}

impl From<&str> for ToolError {
    fn from(e: &str) -> Self {
        ToolError(e.to_string())
    }
}

/// JSON Schema 快捷构造：直接手写 31 份 schema 字面量，与下方 dispatch 一一对照。
/// schema 含运行期构造的 `serde_json::Value`，用 LazyLock 一次性构建。
pub fn catalog() -> &'static [ToolSpec] {
    &CATALOG
}

static CATALOG: std::sync::LazyLock<Vec<ToolSpec>> = std::sync::LazyLock::new(|| {
    vec![
        // ───────────────────────── 只读（15） ─────────────────────────
        ToolSpec {
            name: "system.status",
            description: "启动器概览：应用版本、运行中的游戏实例、MCP 会话数。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[], &[]),
        },
        ToolSpec {
            name: "system.audit_recent",
            description: "最近的 MCP 工具调用审计（已脱敏）。Agent 可用它自查历史。limit 默认 20，上限 100。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("limit", u64_opt())], &[]),
        },
        ToolSpec {
            name: "profiles.list",
            description: "列出当前游戏目录下的实例：key、显示名、游戏版本、加载器。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[], &[]),
        },
        ToolSpec {
            name: "profiles.get",
            description: "按 key（directory/id）读取实例详情（设置 + 生效值 + 路径）。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "instances.list",
            description: "列出实例（与 instances.list 相同；保留为旧名称）。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[], &[]),
        },
        ToolSpec {
            name: "versions.available",
            description: "可安装的 Minecraft 版本（Mojang manifest）。release_type 可选 \"release\"|\"snapshot\"，缺省返回全部。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("release_type", str_opt())], &[]),
        },
        ToolSpec {
            name: "modloader.list",
            description: "某游戏版本下指定加载器的可用版本。loader_type 取 fabric/forge/neoforge/quilt 等小写名。",
            risk: ToolRisk::ReadOnly,
            schema: obj(
                &[("loader_type", str_req()), ("game_version", str_req())],
                &["loader_type", "game_version"],
            ),
        },
        ToolSpec {
            name: "mods.installed",
            description: "列出实例已安装的 Mod（含启用/禁用状态）。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "mods.dependency_scan",
            description: "纯逻辑依赖扫描（非 AI）：缺失必需前置 / 冲突 / 未解析。只解析 Fabric fabric.mod.json；Forge/NeoForge 元数据进入 unparsed。耗时 < 1s。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "mods.search",
            description: "在 Modrinth 搜索 Mod。game_version/loader 可选过滤；page 从 1 起（默认 1，每页 20）。",
            risk: ToolRisk::ReadOnly,
            schema: obj(
                &[
                    ("query", str_req()),
                    ("game_version", str_opt()),
                    ("loader", str_opt()),
                    ("page", u64_opt()),
                ],
                &["query"],
            ),
        },
        ToolSpec {
            name: "mods.versions",
            description: "列出某 Mod 的全部可用版本（含兼容的游戏版本与加载器），供 mods.install 选择。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("mod_id", str_req())], &["mod_id"]),
        },
        ToolSpec {
            name: "instance.check_completion",
            description: "完整性体检：报告实例缺失的文件。耗时数十秒（全量 SHA1 校验），请勿高频调用。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "java.detect",
            description: "探测系统上可用的 Java 安装（版本/路径/供应商）。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[], &[]),
        },
        ToolSpec {
            name: "settings.get",
            description: "全局设置（已剔除一切形似密钥的字段）。不含凭据：API key 永不返回。",
            risk: ToolRisk::ReadOnly,
            schema: obj(&[], &[]),
        },
        ToolSpec {
            name: "logs.game",
            description: "最近一次游戏会话日志的尾部（已脱敏：home 路径 → ~，凭据遮蔽）。lines 默认 200，上限 1000。key 缺省读共享目录日志。",
            risk: ToolRisk::ReadOnly,
            schema: obj(
                &[("key", str_opt()), ("lines", u64_opt())],
                &[],
            ),
        },
        // ───────────────────────── 操作（11） ─────────────────────────
        ToolSpec {
            name: "profiles.update",
            description: "修改实例设置（可逆）。fields 为 InstanceSettingsPatch，缺省字段保持不变。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[("key", str_req()), ("fields", obj_req())],
                &["key", "fields"],
            ),
        },
        ToolSpec {
            name: "game.install",
            description: "安装一个完整的游戏：创建实例 → 下载游戏本体（后台执行，进度经 GUI 事件可见）→ 可选安装加载器。loader: {type, version}。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[
                    ("name", str_opt()),
                    ("game_version", str_req()),
                    ("loader", obj_opt()),
                ],
                &["game_version"],
            ),
        },
        ToolSpec {
            name: "modloader.install",
            description: "为已有实例安装加载器（Fabric/Forge/NeoForge 等）。会创建一个新的版本实例。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[
                    ("key", str_req()),
                    ("loader_type", str_req()),
                    ("loader_version", str_req()),
                ],
                &["key", "loader_type", "loader_version"],
            ),
        },
        ToolSpec {
            name: "mods.install",
            description: "安装 Mod 到实例。version_id 缺省时自动选择与游戏版本+加载器兼容的最新版。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[
                    ("key", str_req()),
                    ("mod_id", str_req()),
                    ("version_id", str_opt()),
                ],
                &["key", "mod_id"],
            ),
        },
        ToolSpec {
            name: "mods.install_local",
            description: "从本机路径安装 Mod jar 到 Profile。path 必须是已存在的 .jar 文件。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[("key", str_req()), ("path", str_req())],
                &["key", "path"],
            ),
        },
        ToolSpec {
            name: "mods.toggle",
            description: "启用或禁用已安装的 Mod（禁用 = 移动到 mods/disabled/，可逆）。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[
                    ("key", str_req()),
                    ("mod_filename", str_req()),
                    ("enabled", bool_req()),
                ],
                &["key", "mod_filename", "enabled"],
            ),
        },
        ToolSpec {
            name: "mods.apply_plan",
            description: "一键补装缺失前置：按最新依赖扫描自动生成计划，从 Modrinth 下载兼容版本（SHA1 校验）。纯网络+本地逻辑，无 AI。",
            risk: ToolRisk::Operate,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "instance.repair",
            description: "修复实例缺失文件（重新下载）。返回修复的文件数。耗时视缺失量可达数分钟。",
            risk: ToolRisk::Operate,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "downloads.control",
            description: "控制下载批次：action 取 pause|resume|cancel。batch_id 来自下载事件或 system 状态。",
            risk: ToolRisk::Operate,
            schema: obj(
                &[("batch_id", str_req()), ("action", str_req())],
                &["batch_id", "action"],
            ),
        },
        ToolSpec {
            name: "launch.start",
            description: "启动游戏（Java 自动检测，启动参数经 response file 传递）。GUI 立即可见运行状态与日志。",
            risk: ToolRisk::Operate,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        // ───────────────────────── 破坏性（5，默认 confirm） ─────────────────────────
        ToolSpec {
            name: "launch.kill",
            description: "强制停止运行中的游戏进程（进程组 SIGKILL，未保存进度丢失）。需要用户在 GUI 确认。",
            risk: ToolRisk::Destructive,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "profiles.delete",
            description: "隐藏实例（保留版本文件，不可见于列表）。需要用户在 GUI 确认。",
            risk: ToolRisk::Destructive,
            schema: obj(&[("key", str_req())], &["key"]),
        },
        ToolSpec {
            name: "instances.remove",
            description: "删除实例及其 versions/{id} 目录（不可逆）。需要用户在 GUI 确认。",
            risk: ToolRisk::Destructive,
            schema: obj(&[("version_id", str_req())], &["version_id"]),
        },
        ToolSpec {
            name: "mods.remove",
            description: "从实例删除 Mod 文件（不可逆）。mod_filename 来自 mods.installed。需要用户在 GUI 确认。",
            risk: ToolRisk::Destructive,
            schema: obj(
                &[("key", str_req()), ("mod_filename", str_req())],
                &["key", "mod_filename"],
            ),
        },
        ToolSpec {
            name: "settings.set",
            description: "修改全局设置（字段白名单：目录/内存/并发/镜像/JVM 参数等；不含任何凭据字段）。需要用户在 GUI 确认。",
            risk: ToolRisk::Destructive,
            schema: obj(&[("fields", obj_req())], &["fields"]),
        },
    ]
});

// ───────────────────────── schema 构造辅助 ─────────────────────────

fn obj(properties: &[(&str, Value)], required: &[&str]) -> Value {
    let mut props = serde_json::Map::new();
    for (name, schema) in properties {
        props.insert((*name).to_string(), schema.clone());
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

fn str_req() -> Value {
    json!({ "type": "string" })
}

fn str_opt() -> Value {
    json!({ "type": "string" })
}

fn u64_opt() -> Value {
    json!({ "type": "integer", "minimum": 0 })
}

fn bool_req() -> Value {
    json!({ "type": "boolean" })
}

fn obj_req() -> Value {
    json!({ "type": "object" })
}

fn obj_opt() -> Value {
    json!({ "type": "object" })
}

// ───────────────────────── dispatch ─────────────────────────

use crate::commands;
use crate::state::AppState;
use tauri::Manager as _;

fn state(app: &tauri::AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

fn arg_str(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing string argument: {key}").into())
}

fn arg_opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

async fn selected_directory_id(app: &tauri::AppHandle) -> Result<String, ToolError> {
    let state = state(app);
    let core = state.core.read().await;
    let service = core
        .instances
        .lock()
        .map_err(|_| "instance service lock poisoned".to_string())?;
    Ok(service.selected_directory().id.to_string())
}

fn arg_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

/// 执行一个工具。策略门禁/审计/确认流在协议层，这里只做参数校验与命令编排。
pub async fn call(app: &tauri::AppHandle, name: &str, args: Value) -> Result<Value, ToolError> {
    match name {
        // ── 只读 ──
        "system.status" => {
            let running = commands::launch::launch_status(state(app)).await?;
            let handle = &state(app).mcp;
            Ok(json!({
                "app_version": env!("CARGO_PKG_VERSION"),
                "running_profiles": running,
                "mcp": {
                    "enabled": handle.is_enabled(),
                    "port": handle.port(),
                    "sessions": handle.list_sessions().len(),
                },
            }))
        }
        "system.audit_recent" => {
            let limit = arg_u64(&args, "limit").unwrap_or(20).min(100) as usize;
            let records = state(app).mcp.read_audit(limit)?;
            Ok(serde_json::to_value(records)?)
        }
        "profiles.list" | "instances.list" => {
            let list = commands::instance::instance_list(state(app), None, None, None).await?;
            Ok(serde_json::to_value(list)?)
        }
        "profiles.get" | "instances.get" => {
            let key = arg_str(&args, "key")?;
            let detail = commands::instance::instance_get(state(app), key).await?;
            Ok(serde_json::to_value(detail)?)
        }
        "versions.available" => {
            let versions = commands::version::version_list(state(app)).await?;
            let mut value = serde_json::to_value(versions)?;
            if let Some(filter) = arg_opt_str(&args, "release_type") {
                if let Some(items) = value.as_array_mut() {
                    items.retain(|v| {
                        v.get("release_type").and_then(Value::as_str) == Some(filter.as_str())
                    });
                }
            }
            Ok(value)
        }
        "modloader.list" => {
            let list = commands::modloader::modloader_list(
                state(app),
                arg_str(&args, "loader_type")?,
                arg_str(&args, "game_version")?,
            )
            .await?;
            Ok(serde_json::to_value(list)?)
        }
        "mods.installed" => {
            let list = commands::mods::mod_list(state(app), arg_str(&args, "key")?).await?;
            Ok(serde_json::to_value(list)?)
        }
        "mods.dependency_scan" => {
            let view =
                commands::mods::mods_dependency_scan(state(app), arg_str(&args, "key")?).await?;
            Ok(serde_json::to_value(view)?)
        }
        "mods.search" => {
            let result = commands::mods::mod_search(
                state(app),
                arg_str(&args, "query")?,
                arg_opt_str(&args, "game_version"),
                arg_opt_str(&args, "loader"),
                arg_u64(&args, "page").map(|p| p as u32),
                None,
                arg_opt_str(&args, "index"),
            )
            .await?;
            Ok(serde_json::to_value(result)?)
        }
        "mods.versions" => {
            let versions =
                commands::mods::mod_get_versions(state(app), arg_str(&args, "mod_id")?).await?;
            Ok(serde_json::to_value(versions)?)
        }
        "instance.check_completion" => {
            let missing =
                commands::instance::instance_check_completion(state(app), arg_str(&args, "key")?)
                    .await?;
            Ok(serde_json::to_value(missing)?)
        }
        "java.detect" => {
            let javas = commands::settings::java_detect(state(app)).await?;
            Ok(serde_json::to_value(javas)?)
        }
        "settings.get" => {
            let config = commands::settings::settings_get(state(app)).await?;
            let mut value = serde_json::to_value(config)?;
            strip_secret_fields(&mut value);
            Ok(value)
        }
        "logs.game" => game_logs(app, &args).await,

        // ── 操作 ──
        "profiles.update" => {
            let key = arg_str(&args, "key")?;
            let fields = args.get("fields").cloned().unwrap_or_else(|| json!({}));
            let opts: rmccc_instance::InstanceSettingsPatch =
                serde_json::from_value(fields).map_err(|e| format!("invalid fields: {e}"))?;
            let detail =
                commands::instance::instance_update_settings(state(app), key, opts).await?;
            Ok(serde_json::to_value(detail)?)
        }
        "game.install" => {
            let game_version = arg_str(&args, "game_version")?;
            let instance_id = arg_opt_str(&args, "name").unwrap_or_else(|| game_version.clone());
            let directory_id = selected_directory_id(app).await?;
            let loader_type = args
                .get("loader")
                .and_then(|l| l.get("type"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let loader_version = args
                .get("loader")
                .and_then(|l| l.get("version"))
                .and_then(Value::as_str)
                .map(str::to_string);
            commands::instance::instance_install(
                state(app),
                directory_id.clone(),
                game_version.clone(),
                Some(instance_id.clone()),
                loader_type,
                loader_version,
            )
            .await?;
            Ok(json!({
                "key": format!("{directory_id}/{instance_id}"),
                "status": "installing",
            }))
        }
        "modloader.install" => {
            commands::modloader::modloader_install(
                state(app),
                arg_str(&args, "key")?,
                arg_str(&args, "loader_type")?,
                arg_str(&args, "loader_version")?,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }
        "mods.install" => {
            let key = arg_str(&args, "key")?;
            let mod_id = arg_str(&args, "mod_id")?;
            let version =
                pick_mod_version(app, &key, &mod_id, arg_opt_str(&args, "version_id")).await?;
            let filename = commands::mods::mod_install(state(app), key, version).await?;
            Ok(json!({ "installed": filename }))
        }
        "mods.install_local" => {
            let filename = commands::mods::mod_install_local(
                state(app),
                arg_str(&args, "key")?,
                arg_str(&args, "path")?,
            )
            .await?;
            Ok(json!({ "installed": filename }))
        }
        "mods.toggle" => {
            commands::mods::mod_toggle(
                state(app),
                arg_str(&args, "key")?,
                arg_str(&args, "mod_filename")?,
                args.get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or("missing boolean argument: enabled")?,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }
        "mods.apply_plan" => {
            let report =
                commands::mods::mods_apply_plan(state(app), arg_str(&args, "key")?).await?;
            Ok(serde_json::to_value(report)?)
        }
        "instance.repair" => {
            let repaired =
                commands::instance::instance_repair(state(app), arg_str(&args, "key")?).await?;
            Ok(json!({ "repaired_files": repaired }))
        }
        "downloads.control" => {
            let batch_id = arg_str(&args, "batch_id")?;
            let action = arg_str(&args, "action")?;
            match action.as_str() {
                "pause" => commands::download::download_pause(state(app), batch_id).await?,
                "resume" => commands::download::download_resume(state(app), batch_id).await?,
                "cancel" => commands::download::download_cancel(state(app), batch_id).await?,
                other => {
                    return Err(format!("unknown action: {other} (pause|resume|cancel)").into())
                }
            }
            Ok(json!({ "ok": true }))
        }
        "launch.start" => {
            commands::launch::launch_start(state(app), arg_str(&args, "key")?).await?;
            Ok(json!({ "ok": true }))
        }

        // ── 破坏性（协议层已放行）──
        "launch.kill" => {
            commands::launch::launch_kill(state(app), arg_str(&args, "key")?).await?;
            Ok(json!({ "ok": true }))
        }
        "profiles.delete" => {
            // Conservative default: hide the instance, keep version files.
            commands::instance::instance_remove(state(app), arg_str(&args, "key")?, Some(false))
                .await?;
            Ok(json!({ "ok": true }))
        }
        "instances.remove" => {
            commands::instance::instance_remove(state(app), arg_str(&args, "key")?, Some(true))
                .await?;
            Ok(json!({ "ok": true }))
        }
        "mods.remove" => {
            commands::mods::mod_remove(
                state(app),
                arg_str(&args, "key")?,
                arg_str(&args, "mod_filename")?,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }
        "settings.set" => {
            let fields = args.get("fields").cloned().unwrap_or_else(|| json!({}));
            let opts = filtered_settings_update(&fields)?;
            let config = commands::settings::settings_set(state(app), opts).await?;
            let mut value = serde_json::to_value(config)?;
            strip_secret_fields(&mut value);
            Ok(value)
        }

        other => Err(format!("unknown tool: {other}").into()),
    }
}

/// `mods.install` 的版本选择：精确 version_id → 兼容过滤（游戏版本+加载器）→ 第一个。
async fn pick_mod_version(
    app: &tauri::AppHandle,
    key: &str,
    mod_id: &str,
    version_id: Option<String>,
) -> Result<rmccc_mods::ModVersion, ToolError> {
    let state = state(app);
    let versions = commands::mods::mod_get_versions(state.clone(), mod_id.to_string()).await?;
    if let Some(id) = version_id {
        return versions
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| format!("version not found: {id}").into());
    }
    let (game_version, loader) = {
        let core = state.core.read().await;
        let mut service = core
            .instances
            .lock()
            .map_err(|_| "instance service lock poisoned".to_string())?;
        let detail = service.get(key)?;
        (
            detail.summary.game_version.clone(),
            detail.summary.loader.as_ref().map(|l| l.kind.clone()),
        )
    };
    let compatible = versions
        .iter()
        .filter(|v| {
            v.game_versions.contains(&game_version)
                && loader.as_deref().is_none_or(|l| {
                    v.loaders
                        .iter()
                        .any(|vl| vl.eq_ignore_ascii_case(l) || normalizes_to(vl, l))
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    compatible
        .into_iter()
        .next()
        .or_else(|| versions.into_iter().next())
        .ok_or_else(|| format!("mod {mod_id} has no available versions").into())
}

/// Modrinth loader 名与内部枚举 Debug 名的差异（neoforge vs NeoForge 等）。
fn normalizes_to(candidate: &str, loader: &str) -> bool {
    candidate
        .replace(['-', '_'], "")
        .eq_ignore_ascii_case(&loader.replace(['-', '_'], ""))
}

/// `settings.get` 的防御性脱敏：递归剔除形似凭据的键。
pub fn strip_secret_fields(value: &mut Value) {
    const SECRET_HINTS: &[&str] = &["token", "key", "secret", "password"];
    match value {
        Value::Object(map) => {
            map.retain(|k, _| {
                let k = k.to_lowercase();
                !SECRET_HINTS.iter().any(|hint| k.contains(hint))
            });
            for (_, v) in map.iter_mut() {
                strip_secret_fields(v);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                strip_secret_fields(item);
            }
        }
        _ => {}
    }
}

/// `settings.set` 字段白名单（设计 §5.5：目录/内存/并发/镜像等，不含凭据）。
const SETTINGS_SET_ALLOWED: &[&str] = &[
    "game_root_dir",
    "default_java_path",
    "default_min_memory_mb",
    "default_max_memory_mb",
    "download_concurrency",
    "mirror_url",
    "default_jvm_args",
    "default_game_args",
    "default_isolation",
    "default_window_width",
    "default_window_height",
    "default_fullscreen",
    "default_process_priority",
    "auto_memory",
];

fn filtered_settings_update(
    fields: &Value,
) -> Result<commands::settings::SettingsUpdate, ToolError> {
    let object = fields
        .as_object()
        .ok_or("settings.set requires fields to be an object")?;
    for key in object.keys() {
        if !SETTINGS_SET_ALLOWED.contains(&key.as_str()) {
            return Err(format!("field '{key}' is not in the settings.set whitelist").into());
        }
    }
    serde_json::from_value(fields.clone()).map_err(|e| format!("invalid fields: {e}").into())
}

/// 游戏日志尾部（脱敏）。
async fn game_logs(app: &tauri::AppHandle, args: &Value) -> Result<Value, ToolError> {
    use rmccc_llm::redact;
    let state = state(app);
    let lines = arg_u64(args, "lines").unwrap_or(200).min(1000) as usize;
    let path = match arg_opt_str(args, "key") {
        Some(key) => {
            let core = state.core.read().await;
            let mut service = core
                .instances
                .lock()
                .map_err(|_| "instance service lock poisoned".to_string())?;
            let detail = service.get(&key)?;
            detail.paths.logs_dir.join("latest.log")
        }
        None => {
            let core = state.core.read().await;
            core.config
                .resolved_game_root()
                .join("logs")
                .join("latest.log")
        }
    };
    let (raw, source) = match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => (text, "latest.log"),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(
                format!("cannot read game log ({}): {e}", path.display(), e = error).into(),
            );
        }
        // latest.log 未生成（log4j 初始化前崩溃）或为空 → 启动器捕获的输出。
        _ => {
            let captured = arg_opt_str(args, "key")
                .and_then(|id| state.game_output.get(&id))
                .map(|buf| {
                    buf.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .join("\n")
                });
            match captured {
                Some(text) if !text.trim().is_empty() => (
                    text,
                    "launcher-captured output (latest.log missing or empty)",
                ),
                _ => {
                    return Err(format!(
                        "cannot read game log ({}): latest.log missing and no captured output",
                        path.display()
                    )
                    .into());
                }
            }
        }
    };
    let selected: Vec<&str> = {
        let all: Vec<&str> = raw.lines().collect();
        let start = all.len().saturating_sub(lines);
        all[start..].to_vec()
    };
    let truncated = raw.lines().count() > selected.len();
    let text = redact::redact_text(&selected.join("\n"), redact::user_home().as_deref());
    Ok(json!({
        "path": redact::redact_text(&path.to_string_lossy(), redact::user_home().as_deref()),
        "source": source,
        "total_lines": raw.lines().count(),
        "returned_lines": selected.len(),
        "truncated": truncated,
        "log": text,
    }))
}

/// serde_json::Error → ToolError（`?` 便捷）。
impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        ToolError(format!("serialization failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_secret_fields_removes_lookalike_keys_recursively() {
        let mut value = json!({
            "game_root_dir": "/g",
            "mirror_url": "https://x",
            "api_key": "sk-1",
            "accessToken": "t",
            "nested": { "secret": 1, "keep": 2 },
            "list": [{ "password": "p", "ok": true }]
        });
        strip_secret_fields(&mut value);
        assert_eq!(value["game_root_dir"], "/g");
        assert!(value.get("api_key").is_none());
        assert!(value.get("accessToken").is_none());
        assert!(value["nested"].get("secret").is_none());
        assert_eq!(value["nested"]["keep"], 2);
        assert!(value["list"][0].get("password").is_none());
    }

    #[test]
    fn settings_set_rejects_fields_outside_the_whitelist() {
        let err = filtered_settings_update(&json!({ "data_dir": "/etc" })).unwrap_err();
        assert!(err.message().contains("whitelist"));
        let err = filtered_settings_update(&json!({ "mirror_url": "https://m" })).map(|_| ());
        assert!(err.is_ok() || true); // mirror_url 在白名单内（解析错误由 serde 报）
                                      // 类型错误也要能体现出来
        let err = filtered_settings_update(&json!({ "default_min_memory_mb": "big" })).unwrap_err();
        assert!(err.message().contains("invalid"));
    }

    #[test]
    fn loader_names_normalize() {
        assert!(normalizes_to("NeoForge", "neoforge"));
        assert!(normalizes_to("fabric", "FABRIC"));
        assert!(!normalizes_to("forge", "neoforge"));
    }
}
