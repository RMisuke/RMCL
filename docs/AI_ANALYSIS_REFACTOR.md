# RMCCC AI 日志分析重构方案（ADR-013 S2 重设计）

> 版本：v1.1（2026-11-04，**已实现**；实施记录与决策见 §11）。
> 定位：本文是 **AI 日志分析（S2）** 的实现级重构设计，细化并修正
> `docs/AI_AND_MCP_ARCHITECTURE.md` §4.2 与 `docs/adr/ADR-013` 的 S2 切片。
> **不改变架构边界**：仍是「轻 AI」——单次调用、无编排循环、无工具注册表、
> 无会话状态机；外部 Agent 经 MCP 的路线不受影响，因此**不需要新 ADR**。
> 实现落地后需回填 §4.2 的偏差说明与 `docs/README.md` 索引。

---

## 0. 一句话目标

把「一键日志分析」从**全局单槽、按文件路径取日志、可能永久卡住、把 12KB 原始日志尾部整段丢给模型**的现状，
重构为**按运行（run）绑定、日志新鲜度可判定、终止事件有保证、发送前做确定性消化、提示词强约束**的可靠子系统。

---

## 1. 审计结论（现状问题 + 证据）

代码路径：`src/stores/ai.ts` · `src/components/LogDrawer.tsx` · `src/stores/launch.ts` · `src/stores/init.ts`
· `src-tauri/src/commands/ai.rs` · `src-tauri/src/commands/launch.rs` · `src-tauri/src/state.rs`
· `src-tauri/src/mcp/tools.rs` · `crates/rmccc-llm/src/llm.rs`。

| # | 问题 | 根因 | 证据 | 用户可见影响 |
|---|------|------|------|------|
| P1 | 取消/中断后**永久卡在"分析中"** | 取消分支不发送任何终止事件，channel 直接关闭 | `llm.rs:378-386`（`Ok(Err(Cancelled)) => None`）；`ai.rs:400-427`（`while let Some(...)` 结束后不发 `AI_DONE`/`AI_ERROR`）；`ai.ts:79-84`（`cancelAnalysis` 不改本地状态） | 点"取消"后转圈不消失；上一次卡住的状态会残留到下一次 |
| P2 | **游戏退出后在途分析不被清理** | 无任何逻辑把 run 生命周期与 `ai_cancels` 关联 | `state.rs:58-61`（`ai_cancels` 仅按 `analysis_id` 索引）；`launch.rs:144-164`（`on_exit` 只发 `LAUNCH_EXITED`，不触碰分析） | 退出后旧分析仍在流式，或卡死状态跨运行残留 |
| P3 | **使用上一次的日志** | 日志来源按文件路径解析，无"属于本次运行"的判定 | `ai.rs:263-291`：优先磁盘 `latest.log`，仅当其为空/缺失才回退 `game_output`。启动早期 log4j 尚未重写 `latest.log`，旧文件仍在且非空 → 读到**上一轮**日志 | 新开的游戏分析出旧崩溃；"每次打开游戏用上一次日志" |
| P4 | 分析结果是**全局单槽**，不按运行/实例隔离 | `AnalysisState` 无 `runId`；抽屉不做 profile 过滤 | `ai.ts:11-19,26`；`ai.rs:60-67`（`AiAnalysisHandle` 只有 `analysis_id/sent_lines`）；`LogDrawer.tsx` `const analysis = () => aiStore.analysis()`（无过滤） | 看实例 B 时显示实例 A 的分析；新运行不复位旧结果 |
| P5 | **提示词/token 浪费、效率低** | 直接发送最多 12,000 字符**原始尾部**，无消化；未设 `max_tokens`/`temperature` | `ai.rs:35`（`ANALYSIS_LOG_MAX_CHARS=12_000`）；`ai.rs:41-46`（通用系统提示，无长度/结构约束）；`ai.rs:387-390`（`ChatRequest::new` 未带采样/上限参数）；设计 §4.2 要求的"崩溃优先截取 ERROR 附近窗口"**从未实现** | 大量常规 INFO 噪声入 prompt，输出冗长、慢、贵 |
| P6 | MCP `game_logs` 与 ai.rs **逻辑重复且更差** | 各自实现一套"latest.log→回退"，且整文件读入 | `mcp/tools.rs:746-814`（`read_to_string` 整读几十 MB；同样的 stale 回退） | 崩溃后 MCP 读全量日志占内存；两条路径行为漂移 |
| P7 | 无"首字节超时"，死连接要等满 180s | 只有整轮 `timeout=180s` | `llm.rs:369-390`（`tokio::time::timeout(timeout, run_stream)`）；`ai.rs:39`（`ANALYSIS_TIMEOUT_SECS=180`）；设计 §4.1 承诺"首字节 30s"未实现 | 网络黑洞时长时间无反馈 |
| P8 | 状态机缺 `cancelled`，UI 无法区分"用户取消"与"出错" | `state: "running"\|"done"\|"error"` | `ai.ts:15` | 取消被当成静默/错误 |

> 说明：P1 是"卡加载中"的**直接根因**；P3+P4 共同造成"用上一次日志/上一次结果"。
> P5 是"讲重点、省 token、提效率"的落点。其余为连带健壮性问题。

---

## 2. 重构目标（可度量）

1. **永不卡死**：任何一次分析都**恰好**以一个终止事件收尾（完成 / 出错 / 取消）；取消后 UI 在 1 帧内脱离"分析中"。
2. **日志必属本次运行**：分析输入绑定 `run_id`；无法证明新鲜时**明确报错**而不是回退旧日志。
3. **按运行隔离**：分析结果与 `run_id` 绑定；新运行开始即复位旧结果；跨实例不串台。
4. **省 token / 讲重点**：发送前做确定性"日志消化"，典型崩溃场景 payload 从 ~12KB 降到 **≤5KB**（约省 50%+ token），且**信噪比提升**（优先崩溃报告/异常栈/ERROR 窗口）。
5. **更快收敛**：`max_tokens=700` + `temperature=0.2` + 首字节 30s 超时；输出结构化 ≤200 字。
6. **单一实现**：ai.rs 与 MCP `game_logs` 共用同一个"新鲜日志解析器"，删除重复的整文件读。
7. **门禁不退化**：IPC 命令数不变（仍 133）；`cargo test` 覆盖取消终止、消化、新鲜度判定；主二进制 strip 后仍 < 20 MiB。

---

## 3. 目标架构总览

```
launch_start(profile)                       LAUNCH_STARTED{profile,run_id,pid}
   │  mint run_id, 记录 RunRecord{run_id,pid,started_at_ms}
   │  清空 game_output[profile]（既有）
   ▼
TauriProcessListener.on_log ──► game_output[profile]（run 内累积，cap 2000，既有）
   │
on_exit ──► LAUNCH_EXITED{profile,run_id,exit_code,exit_type}
   │            │
   │            └─(前端) 若该 run 有在途分析：保留让其收尾（崩溃正是需要分析时），
   │                     但保证终止事件；新 run 开始才取消旧 run 的分析
   ▼
ai_analyze_log{key, run_id}
   │  1) resolve_fresh_log(profile, run) ── 新鲜度判定（run-scoped）
   │  2) digest_log(raw) ── 确定性消化（崩溃报告/异常栈/ERROR 窗口/去噪）
   │  3) redact ── 脱敏（既有）
   │  4) ChatRequest{ system=强约束提示, user=上下文+digest, max_tokens, temperature }
   │  5) 登记 AnalysisRecord{analysis_id, run_id, cancel}
   ▼
provider.stream_chat(cancel)
   │  终止保证：Done | Error | Cancelled（新增 Cancelled 变体）
   ▼
run_analysis ── 每个终止分支都 emit 对应事件（含 cancelled）──► 前端按 run_id 落位
```

**关键不变量（重构后必须成立）**

- **I1 终止保证**：`stream_chat` 返回的流，`recv()` 最终必产出 `Done`/`Error`/`Cancelled` 之一后关闭；`run_analysis` 对三者分别 emit `ai://done{reason}` / `ai://error`。绝不"静默关闭"。
- **I2 新鲜度**：分析用的日志文本要么来自 `game_output`（天然 run 内），要么来自 `mtime ≥ run.started_at` 的 `latest.log`；否则返回 `NoFreshLog` 错误。
- **I3 运行绑定**：`AnalysisState.runId` 必填；UI 只展示 `runId === 当前 profile 的 run` 的分析。
- **I4 单一解析器**：ai.rs 与 MCP `game_logs` 都经 `resolve_fresh_log`；不存在第二套 latest.log 回退逻辑。

---

## 4. 详细设计

### 4.1 运行记录与日志新鲜度（治 P3/P6，落地 I2/I4）

**新增运行记录**（`AppState`）：

```rust
// state.rs
pub struct RunRecord {
    pub run_id: String,       // uuid simple，launch_start 时铸造
    pub pid: u32,
    pub started_at_ms: u64,   // SystemTime::now() 的 unix ms
    pub exited: bool,         // on_exit 置 true（记录保留到下次 launch 覆盖）
}
pub runs: Arc<dashmap::DashMap<String, RunRecord>>, // key = profile_id（保留最近一次 run）
```

- `launch_start`：铸造 `run_id`，`runs.insert(profile, RunRecord{..})`，并在 `LAUNCH_STARTED` payload 增加 `run_id`。
- `on_exit`：`runs.get_mut(profile).exited = true`，`LAUNCH_EXITED` payload 增加 `run_id`。
- `running_games` 保持不变（kill 路径不动）；`runs` 是**附加**记录，退出后仍保留供事后分析，直到同 profile 下次 launch 覆盖。

**共享日志解析器**（新增 `src-tauri/src/game_log.rs`，ai.rs 与 mcp/tools.rs 共用）：

```rust
pub enum LogSourceKind { Captured, DiskFresh, DiskUnbound /* 无 run 记录时的尽力而为 */ }

pub struct ResolvedLog {
    pub text: String,          // 已按读取窗口截尾（未消化、未脱敏）
    pub kind: LogSourceKind,
    pub run_id: Option<String>,
    pub total_lines: usize,
}

pub enum LogError { NoFreshLog, Io(std::io::Error) }

/// 新鲜度优先：captured(run 内) > disk(mtime≥started_at) > 无 run 时 disk 尽力而为。
pub async fn resolve_fresh_log(
    state: &AppState,
    profile_id: Option<&str>,
    read_window_chars: usize,
) -> Result<ResolvedLog, LogError>;
```

判定规则（`profile_id = Some(key)`）：

1. 取 `run = runs.get(key)`、`captured = game_output[key].join("\n")`。
2. **若 `captured` 非空 → 用 captured**（`kind=Captured`，天然属于本 run：`launch_start` 已清空、`on_log` 只累积本轮）。这修掉 P3：不再优先磁盘旧文件。
3. 否则读磁盘 `latest.log` 尾部（沿用 `ai.rs` 现有 off-thread `spawn_blocking` 尾读，**不整读**）：
   - 若 `run` 存在且 `mtime_ms ≥ run.started_at_ms - 2000`（容忍时钟偏移）→ `kind=DiskFresh`；
   - 若 `run` 存在但 mtime 更早 → 判定为**旧日志**，返回 `LogError::NoFreshLog`（不回退旧内容）；
   - 若 `run` 不存在（App 重启后分析上次崩溃）→ `kind=DiskUnbound`，尽力而为（明确标注来源，不作为"本次运行"）。
4. `captured` 与磁盘都空/陈旧 → `NoFreshLog`，前端人话："本次运行尚未产生可分析日志，请稍后或先启动一次游戏"。

`profile_id = None`（共享目录）：无 run 绑定，走磁盘尾读，`kind=DiskUnbound`。

**MCP 收敛（P6）**：`mcp/tools.rs::game_logs` 改为调用 `resolve_fresh_log`，删除 `read_to_string` 整读与自有回退分支；`lines` 参数作用于 `ResolvedLog.text` 的尾部选取。返回体新增 `source`（Captured/DiskFresh/DiskUnbound）便于 Agent 判断新鲜度。

### 4.2 取消 / 终止语义（治 P1/P8，落地 I1）

**llm.rs**：

- `LlmEvent` 新增 `Cancelled` 变体（终止语义显式化）。
- `stream_chat` 的 spawned 收尾：`Ok(Err(LlmError::Cancelled)) => Some(LlmEvent::Cancelled)`（不再是 `None`）。
- `collect()` 遇到 `Cancelled` 返回 `Err(LlmError::Cancelled)`（保持一次性调用方语义）。

**ai.rs `run_analysis`**：对三种终止都 emit：

```rust
LlmEvent::Done{..}      => emit ai://done { analysis_id, reason: "complete" },
LlmEvent::Cancelled     => emit ai://done { analysis_id, reason: "cancelled" },
LlmEvent::Error(e)      => emit ai://error { analysis_id, message },
```

- `AiDonePayload` **扩展**一个字段 `reason: String`（`"complete" | "cancelled"`）。**不新增事件名**（保持 `ai://delta|done|error` 三条，事件注册表不变）。
- `run_analysis` 结束后**无论哪条路径**都 `remove_analysis(&analysis_id)`（幂等）。

**前端 `cancelAnalysis`**：乐观置态 + 后端兜底：

```ts
async function cancelAnalysis() {
  const cur = analysis();
  if (cur?.state === "running") {
    setAnalysis({ ...cur, state: "cancelled" });   // 乐观：立即脱离转圈
    await ai.cancelAnalysis(cur.analysisId);        // 后端取消流；ai://done{cancelled} 幂等确认
  }
}
```

`AnalysisState.state` 增加 `"cancelled"`；`ai://done` 的 `reason==="cancelled"` 也映射到该态（防止乐观置态与后端竞态时回退）。

### 4.3 分析生命周期绑定（治 P2/P4，落地 I3）

**后端登记结构升级**：`ai_cancels: HashMap<analysis_id, CancelToken>` → `ai_analyses: HashMap<analysis_id, AnalysisRecord>`：

```rust
struct AnalysisRecord { cancel: CancelToken, profile_id: Option<String>, run_id: Option<String> }
```

- `ai_analyze_log{key, run_id}` 登记 `profile_id/run_id`；handle 回传 `analysis_id, run_id, sent_lines, omitted_lines, source`。
- 新增内部函数 `cancel_analyses_for(profile_id)`：取消并摘除该 profile 名下所有在途分析（每个都触发 `Cancelled` → `ai://done{cancelled}`）。
- **调用点**：`launch_start` 铸造新 `run_id` 后、清空 `game_output` 前，调用 `cancel_analyses_for(profile)`——**新运行开始即取消上一运行的分析**（旧分析对新运行无意义）。
- **游戏退出**：**不**自动取消（崩溃退出正是需要分析完成的时刻）；靠 I1 终止保证 + 首字节/整轮超时确保它不会卡死。抽屉关闭/切换实例时由前端取消（见 4.6）。

> 取舍见 §8：退出即杀 vs 退出保留。本方案选"退出保留、新运行才杀"，因为分析对象是**已固定的日志文本**，退出不会使其失效，而崩溃后自动/手动分析是核心价值。

### 4.4 日志消化与 token 预算（治 P5，落地目标 4/5）

新增 `src-tauri/src/commands/ai.rs` 内的 `digest_log`（或独立 `ai_digest.rs`，纯函数、可单测）：

```rust
pub struct Digest { pub text: String, pub sent_lines: usize, pub omitted_lines: usize, pub has_crash_report: bool }
pub fn digest_log(raw: &str, budget_chars: usize) -> Digest;
```

**确定性消化算法（无 AI、可测）**，按优先级组装，最后按原顺序回填并截到 `budget_chars`：

1. **崩溃报告块**：若存在 `---- Minecraft Crash Report ----`，截取到下一个 `----` 或 `#@!@# Game crashed!` 之间，原样保留（上限 ~3KB）。
2. **异常/错误行 + 上下文**：扫描命中 `ERROR`/`FATAL`/`Caused by:`/`Exception`/`Error:`/`at <pkg>.`（栈帧）/`Game crashed` 的行，各保留 ±2 行上下文窗口。
3. **去噪折叠**：连续 >3 行的相似 INFO（如 mod 初始化、注册表 dump、资源重载）折叠为 `…（N 行常规日志已省略）`；连续重复行去重。
4. **尾部保真**：始终附加最后 ~40 行原文（致命错误几乎总在末尾）。
5. 统计 `omitted_lines`，在 user 消息里透明标注"已省略 N 行常规日志"。

**预算常量**（替换现有 `ANALYSIS_LOG_MAX_CHARS`）：

| 常量 | 值 | 说明 |
|------|----|------|
| `ANALYSIS_READ_WINDOW_CHARS` | 200_000 | off-thread 尾读窗口（消化前的原料，覆盖大崩溃日志） |
| `ANALYSIS_DIGEST_MAX_CHARS` | 5_000 | 消化后**实际发送**上限（较原 12,000 降 ~58%） |
| `ANALYSIS_MAX_OUTPUT_TOKENS` | 700 | 约束输出长度，防跑题/冗长 |
| `ANALYSIS_TEMPERATURE` | 0.2 | 诊断任务要稳定、克制 |
| `ANALYSIS_FIRST_BYTE_TIMEOUT_SECS` | 30 | 首字节看门狗（治 P7） |
| `ANALYSIS_TOTAL_TIMEOUT_SECS` | 120 | 整轮（从 180 收紧） |

**首字节超时（P7）**：在 `run_stream`/`stream_response` 增加"首个 `Delta` 之前"的独立看门狗——若 30s 内无任何 chunk/delta，返回 `LlmError::Timeout`（人话："AI 服务无响应（首字节超时）"）。实现方式：`tokio::select!` 首个 chunk 与 `sleep(30s)`。

### 4.5 提示词重设计（治 P5，目标 5）

**系统提示（强约束、结构化、简体中文）**：

```text
你是 Minecraft 启动器的日志诊断助手。只依据用户提供的脱敏日志摘要作答；
禁止编造或推测日志中不存在的内容；禁止复述/翻译日志原文。
用简体中文，总字数不超过 200 字，严格输出以下三段，不加前言、结语或额外标题：
结论：一句话根因；若日志不足以判断，写“日志不足”并用一句说明缺什么。
关键行：最多 3 条，每条引用日志片段（≤20 字）。
建议：最多 3 条可执行步骤，每条一句话。
```

**用户消息**（上下文 + 消化摘要 + 透明标注）：

```text
实例「{name}」：Minecraft {version}，加载器 {loader}；退出码 {exit_code}（{exit_type}）。
日志来源：{source}；已按崩溃优先消化，省略 {omitted} 行常规日志。
----- 日志摘要（脱敏）-----
{digest}
```

- 退出码/类型来自 `lastExit`（前端已知）或 `RunRecord`；给模型强信号，减少猜测、缩短输出。
- `ChatRequest` 带上 `temperature=0.2`、`max_tokens=700`（现有 `ChatRequest` 已支持这两个字段，仅未使用）。

### 4.6 前端 store 与 UI 状态机（治 P1/P4/P8）

**`ai.ts`**：

- `AnalysisState` 增 `runId: string | null`、`source?: string`、`omittedLines?: number`；`state` 增 `"cancelled"`。
- `analyzeLog(profileId, runId)` 透传 `runId`，落位到 `analysis`（单槽即可，靠 `runId` 过滤）。
- `attachEvents`：
  - `ai://delta`：仅当 `analysisId` 匹配当前槽才追加（既有），**并要求 `runId` 匹配当前展示 run**（防串台）。
  - `ai://done`：按 `reason` 落 `done` 或 `cancelled`。
- 新增 `resetAnalysis()`：置 `null`。调用点：
  - `launch://started`（launchStore）：该 profile 新 run → `resetAnalysis()`（清旧结果）。
  - 抽屉关闭 / 切换实例：`resetAnalysis()`（可选：仅当 `runId` 不属于当前 profile）。
- `cancelAnalysis`：见 4.2（乐观置 `cancelled`）。

**`launch.ts`**：

- 维护 `runIdByProfile: Map<string,string>`，由 `LAUNCH_STARTED{run_id}` 写入、`LAUNCH_EXITED{run_id}` 保留；暴露 `currentRunId(profileId)`。
- `LAUNCH_STARTED` 处理里调用 `aiStore.resetAnalysis()`（新运行复位）。

**`LogDrawer.tsx`**：

- `const analysis = createMemo(() => { const a = aiStore.analysis(); return a && a.runId === launchStore.currentRunId(profileId()) ? a : null; })`——**只显示属于当前 run 的分析**（治 P4 串台）。
- 关闭按钮：若当前 run 有在途分析 → 先 `cancelAnalysis()` 再关（避免后台空转）。
- 新增 `cancelled` 态的 UI 文案（"已取消"）与"重新分析"按钮；`NoFreshLog` 错误走人话提示。
- AI 分析按钮触发时把 `launchStore.currentRunId(profileId())` 一并传入 `analyzeLog`。

**`init.ts`**：崩溃自动开抽屉逻辑不变；**可选**（§10 待确认）在 egress 已批准 + 已配置时，对异常退出自动发起一次绑定该 run 的分析。

### 4.7 类型与 IPC 面变化

| 项 | 变化 | 命令数影响 |
|----|------|-----------|
| `AiAnalysisHandle` | 增 `runId`、`source`、`omittedLines`（ts-rs 重新生成 `types.ts`） | 无（仍 `ai_analyze_log`） |
| `AiDonePayload` | 增 `reason: "complete"\|"cancelled"` | 无（事件名不变） |
| `ai_analyze_log` 入参 | 增可选 `run_id` | 无 |
| `LAUNCH_STARTED/EXITED` payload | 增 `run_id`（inline json，前端 inline 类型同步） | 无 |
| `LlmEvent` | 增 `Cancelled` 变体（crate 内部） | 无 |
| `AppState` | `ai_cancels`→`ai_analyses`（含 profile/run）；新增 `runs` | 无 |

> **本轮 IPC 命令总数不变**（重构当时为 133 条；2026-11-05 现为 **135**，断言在
> `src-tauri/src/ipc_tests.rs:170`）；仅 payload/类型字段扩展，均为 additive。

---

## 5. 具体实现清单（按文件）

**后端**

- `crates/rmccc-llm/src/llm.rs`：`LlmEvent::Cancelled`；`stream_chat` 收尾发 `Cancelled`；`collect` 处理；首字节看门狗（`select!`）；`lib.rs` 导出不变。
- `src-tauri/src/state.rs`：`RunRecord` + `runs`；`ai_cancels`→`ai_analyses`（`AnalysisRecord`）。
- `src-tauri/src/commands/launch.rs`：`launch_start` 铸造 `run_id`、写 `runs`、调用 `cancel_analyses_for`、payload 加 `run_id`；`on_exit` 标 `exited` + payload 加 `run_id`。
- `src-tauri/src/game_log.rs`（新）：`resolve_fresh_log` + 尾读工具（从 ai.rs 迁移）+ `LogSourceKind/LogError`。
- `src-tauri/src/commands/ai.rs`：`digest_log` + 常量替换；`gather_analysis_input` 改用 `resolve_fresh_log` + `digest_log`；系统/用户提示替换；`ChatRequest` 带 `temperature/max_tokens`；`run_analysis` 三终止分支 emit；`AnalysisRecord` 登记/摘除；`cancel_analyses_for`；handle 扩展字段；单测（消化、新鲜度、取消终止）。
- `src-tauri/src/mcp/tools.rs`：`game_logs` 改用 `resolve_fresh_log`，删整读与重复回退。
- `src-tauri/src/events.rs`：`AiDonePayload.reason`。

**前端**

- `src/ipc/types.ts`：由 ts-rs 重新生成（`AiAnalysisHandle`、`AiDonePayload`）。
- `src/ipc/api.ts`：`analyzeLog(key?, runId?)`。
- `src/stores/ai.ts`：状态机扩展（`runId/cancelled/source/omittedLines`）、`resetAnalysis`、乐观取消、事件按 `reason/runId` 落位。
- `src/stores/launch.ts`：`runIdByProfile` + `currentRunId`；`LAUNCH_STARTED` 复位分析。
- `src/components/LogDrawer.tsx`：按 run 过滤 `analysis`；`cancelled`/`NoFreshLog` UI；关闭即取消；传 `runId`。
- `src/i18n/locales/{zh-CN,en}.ts`：`logs.ai.cancelled`、`logs.ai.noFreshLog`、`logs.ai.source.*`、首字节超时等人话文案。
- `src/stores/init.ts`（可选）：崩溃自动分析。

**文档回填（实现后）**：`AI_AND_MCP_ARCHITECTURE.md` §4.2 偏差说明；`docs/README.md` 索引；`PROJECT_STATUS.md` 轮次。

---

## 6. 分阶段实施计划与门禁

| 阶段 | 内容 | 验收 |
|------|------|------|
| R1 终止保证 | llm `Cancelled` + ai.rs 三分支 emit + 前端乐观取消 + `cancelled` 态 | 单测：取消必产终止事件；手测：点"取消"立即脱离转圈 |
| R2 运行绑定 | `RunRecord`/`runs` + payload `run_id` + `ai_analyses` + `cancel_analyses_for` + 前端 `runIdByProfile`/按 run 过滤/复位 | 新运行开始取消旧分析；跨实例不串台 |
| R3 新鲜日志 | `game_log.rs::resolve_fresh_log` + ai.rs 接入 + MCP 收敛 + `NoFreshLog` 人话 | 启动早期分析不再读旧 latest.log；无新鲜日志时明确报错 |
| R4 消化+提示词 | `digest_log` + 预算常量 + 新系统/用户提示 + `temperature/max_tokens` | 崩溃场景 payload ≤5KB；输出 ≤200 字三段式 |
| R5 超时+健壮 | 首字节 30s 看门狗 + 整轮 120s + 错误人话化 | 死连接 30s 内人话报错 |

**每阶段门禁**（AGENTS.md）：`cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` · `pnpm typecheck` · `pnpm test` · `pnpm build`；R1–R5 合并后 `pnpm tauri build --bundles app` 实测 `.app`（strip 后 < 20 MiB），并按 §7 做一轮 macOS 实机验收。

---

## 7. 验收标准

1. **不卡死**：分析中点"取消" → 1 帧内变"已取消"；再次"分析"可正常发起。断网/错 key → 人话错误（非转圈）。
2. **首字节超时**：指向一个只连不发的端点 → ≤30s 报"无响应"。
3. **新鲜度**：启动游戏后立刻开抽屉分析（log4j 尚未写盘）→ 用 captured 或明确 `NoFreshLog`，**绝不**是上一轮 latest.log。
4. **运行隔离**：分析实例 A 后切到实例 B → 抽屉不显示 A 的结果；A 再次启动 → 旧结果复位。
5. **退出策略**：崩溃退出时在途分析能正常收尾并展示（不被误杀）；同实例再次启动 → 上一轮在途分析被取消。
6. **省 token**：制造一次典型崩溃（如缺前置/坏 mod）→ 发送摘要 ≤5KB、含崩溃报告/异常栈、省略常规行有标注；输出三段式 ≤200 字、引用真实日志行、无编造。
7. **MCP 一致**：`logs.game` 返回的 `source` 与 ai 分析一致；大 latest.log 不再整读入内存。
8. **门禁全绿 + IPC 仍 133 + `.app` 体积达标**。

---

## 8. 风险与取舍

- **退出即杀 vs 退出保留在途分析**：用户原话倾向"退出后杀掉"。本方案选**退出保留、新运行才杀**，理由：分析对象是已固定的日志文本，崩溃退出恰是最需要结论的时刻，杀掉反而丢结果；而"卡死"的真正根因是 P1（无终止事件），R1 已根治。若用户坚持"退出即杀"，只需在 `on_exit` 增调 `cancel_analyses_for(profile)`（一行），本文其余不变 → 列入 §10 待确认。
- **captured 优先于磁盘**：`game_output` cap 2000 行，超长运行的完整历史在磁盘 `latest.log`。崩溃诊断只需尾部，2000 行足够；若未来需要全量历史，可在 `resolve_fresh_log` 增"captured 命中崩溃标记则用之，否则并集磁盘新鲜尾部"的策略（当前不做，避免复杂化）。
- **消化误删关键行**：确定性规则可能漏掉非常规格式的错误。缓解：始终保真最后 40 行 + 崩溃报告块整段保留 + `omitted` 透明标注；规则可单测迭代。
- **`reason` 复用 `ai://done`**：不新增事件名以保持注册表稳定；代价是"取消"经 done 通道传递，前端靠 `reason` 区分（已含乐观置态兜底竞态）。
- **体积**：新增逻辑均为纯 Rust 小函数，无新依赖；`runs`/`ai_analyses` 为轻量 map。对 20 MiB 红线无实质影响，仍跑 `cargo bloat` 复核。

---

## 9. 非目标（本次不做）

- 不引入多轮对话 / 会话存储 / 工具调用编排（ADR-013 明确外部 Agent 负责）。
- 不做分析历史列表 / 多次结果对比（单槽 + run 绑定即可满足当前 UX；如需另议）。
- 不改 MCP 工具目录数量与确认流；`game_logs` 仅内部实现收敛。
- 不做非 macOS 平台实测（Windows/Linux 记 deferred，随 §1.4 约束）。
- 不新增 IPC 命令（保持 133）。

---

## 10. 待确认项（请用户决策）

1. **游戏退出时在途分析**：默认「保留至收尾，新运行才取消」；是否改为「退出立即取消」？
2. **崩溃自动分析**：异常退出且已批准外发 + 已配置时，是否**自动**发起一次分析（省一次点击）？默认关，可加设置开关。
3. **发送预算**：`DIGEST_MAX_CHARS=5000` / `max_tokens=700` / 首字节 30s / 整轮 120s 是否合适，或按你常用模型调整？
4. **输出语言**：固定简体中文，还是跟随界面语言（en 时用英文提示）？
5. **消化激进度**：是否要把常规 INFO 折叠得更狠（进一步省 token）还是保留更多上下文（更稳）？默认取中（±2 行窗口 + 尾部 40 行）。

---

## 11. 实施记录（2026-11-04）与最终决策

§10 的待确认项已由用户拍板，最终实现如下（与设计稿的差异以本节为准）：

| 事项 | 最终决策 | 落地 |
|------|----------|------|
| 游戏退出时在途分析 | **保留到收尾**；同实例**新运行开始**才取消旧分析 | `launch.rs` 新运行调 `cancel_analyses_for`；退出不杀 |
| 崩溃自动分析 | **开关，默认关**，位于「设置 · AI」 | `aiStore.autoAnalyzeOnCrash` + `init.ts` 异常退出钩子 |
| 输出详细程度 | **简略 / 适中 / 详细** 三档，位于「设置 · AI」；适中约 300–400 字并给出修复建议 | `Detail` 枚举 + 按档位切换系统提示与 `max_tokens`（500 / 1000 / 1800） |
| 输出语言 | **跟随界面语言**（zh-CN / en） | 前端传 `locale()`；后端 6 套系统提示（2 语言 × 3 档） |
| 日志提交模式 | **标准＝完整日志**；**激进＝仅 ERROR/WARNING（含 FATAL）行**；位于「设置 · AI」 | `LogMode` + `select_log_text`；标准尾部 24k 字符，激进扫描 120k 后筛选、发送上限 6k |

**与设计稿 §4.4 的偏差**：用户把"消化激进度"定义为**用户可选的两档提交模式**，而非固定的崩溃优先消化算法。
因此 `digest_log`（崩溃报告块 + 异常栈窗口 + 去噪折叠）**未采用**，改为：
- 标准：完整日志尾部（`game_log::tail_chars`，24k 字符）；
- 激进：仅保留含 `ERROR` / `WARN` / `FATAL` 的行，并回传 `omittedLines` 透明显示。

**其他落地细节**：
- `LlmEvent::Cancelled` + 首字节 30s 看门狗；取消/超时/错误都发终止事件（`ai://done{reason}` / `ai://error`），前端**乐观置** `cancelled`。
- `RunRecord{run_id,pid,started_at_ms,exited}` + `AppState::runs`；`LAUNCH_STARTED/EXITED` payload 增 `run_id`；
  `game_log::resolve_fresh_log`：`captured`（本轮，优先）→ 磁盘 `latest.log`（仅 `mtime ≥ started_at`）→ 无 run 时 `disk-unbound`。
- MCP `logs.game` 改用同一解析器，删除整文件 `read_to_string` 与重复回退；`source` 字段回传 `captured/disk-fresh/disk-unbound`。
- 温度 0.2；整轮超时 120s；并发上限 2 不变；**IPC 命令数保持 133**（仅扩展 payload/类型）。
- 门禁（本机 Apple Silicon）：`cargo fmt --check` ✅、`cargo clippy --workspace --all-targets -D warnings` ✅、
  `cargo test --workspace` ✅（46 个 test target）、`pnpm typecheck` ✅、`pnpm test` ✅（86）、`pnpm build` ✅。
  顺带修复了 3 处**既有**工具链漂移 lint（`rmccc-config` 的 fold/derivable Default、`external.rs` 的 sort_by、`data_paths.rs` 的 collapsible if），与本功能无关。
