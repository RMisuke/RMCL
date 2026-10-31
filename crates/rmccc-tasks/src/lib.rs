//! Unified task model for every download / installation transaction.
//!
//! A task is what the user sees: one install or download operation with a
//! title, a target, a weighted step timeline, live download counters and
//! terminal result. Download batches are an implementation detail inside a
//! step, never a separate top-level row.
//!
//! The registry is transport agnostic: the Tauri layer forwards
//! [`TaskSnapshot`] values as `task://updated`, while the CLI can consume the
//! same snapshots without depending on Tauri.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use ts_rs::TS;

/// Minimum interval between high-frequency progress snapshots.
pub const PUBLISH_INTERVAL: Duration = Duration::from_millis(100);
/// Number of detail lines included in every snapshot (the full log is
/// available through [`TaskRegistry::log`]).
pub const DETAIL_PREVIEW: usize = 8;
/// Full per-task log capacity.
pub const DETAIL_CAPACITY: usize = 240;
/// Finished tasks kept for the "recent" section.
pub const HISTORY_CAPACITY: usize = 32;
const BROADCAST_CAPACITY: usize = 256;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// What kind of transaction a task represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "types.ts")]
pub enum TaskKind {
    InstanceInstall,
    ModpackInstall,
    ModloaderInstall,
    ModInstall,
    DependencyInstall,
    InstanceRepair,
    ResourceDownload,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InstanceInstall => "instance-install",
            Self::ModpackInstall => "modpack-install",
            Self::ModloaderInstall => "modloader-install",
            Self::ModInstall => "mod-install",
            Self::DependencyInstall => "dependency-install",
            Self::InstanceRepair => "instance-repair",
            Self::ResourceDownload => "resource-download",
        }
    }
}

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "types.ts")]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Lifecycle state of one step in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "types.ts")]
pub enum TaskStepState {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

impl TaskStepState {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Skipped)
    }
}

/// Who initiated the task (attribution, ADR-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "types.ts")]
pub enum TaskOriginKind {
    #[default]
    User,
    Agent,
    Plugin,
}

/// Per-task action availability.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskCapabilities {
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_cancel: bool,
    pub can_retry: bool,
}

/// A terminal or per-step error shown in the task card.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskErrorView {
    pub message: String,
    pub code: Option<String>,
    pub retryable: bool,
}

/// Result payload used by "view instance" / "reveal folder" actions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskResultView {
    pub key: Option<String>,
    pub instance_id: Option<String>,
    pub directory_id: Option<String>,
    pub manual_directory: Option<String>,
    pub name: Option<String>,
    #[ts(type = "number | null")]
    pub installed_files: Option<u64>,
    pub version: Option<String>,
}

/// Live counters for the download currently feeding a task.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskDownloadView {
    #[ts(type = "number")]
    pub completed_files: u64,
    #[ts(type = "number")]
    pub total_files: u64,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// EMA-smoothed bytes per second.
    pub speed_bps: f64,
    /// Remaining seconds; `None` when the total size or speed is unknown.
    #[ts(type = "number | null")]
    pub eta_seconds: Option<u64>,
    pub current_file: Option<String>,
}

/// One step of the visible timeline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskStepView {
    pub id: String,
    pub label: String,
    pub state: TaskStepState,
    pub weight: u32,
    pub percent: f64,
    #[ts(type = "number")]
    pub completed: u64,
    #[ts(type = "number")]
    pub total: u64,
    pub message: Option<String>,
}

/// Immutable snapshot broadcast on every task change.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskSnapshot {
    pub id: String,
    pub kind: TaskKind,
    pub state: TaskState,
    pub title: String,
    pub subtitle: Option<String>,
    /// Instance key (`{directory}/{instance}`), when the task has one.
    pub target: Option<String>,
    /// Kind-specific marker (loader type, mod id, …).
    pub tag: Option<String>,
    pub origin: TaskOriginKind,
    pub origin_label: Option<String>,
    pub steps: Vec<TaskStepView>,
    pub current_step: Option<String>,
    pub current_step_label: Option<String>,
    pub overall_percent: f64,
    pub message: String,
    pub download: Option<TaskDownloadView>,
    pub warnings: Vec<String>,
    pub error: Option<TaskErrorView>,
    pub result: Option<TaskResultView>,
    pub capabilities: TaskCapabilities,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub started_at: u64,
    #[ts(type = "number | null")]
    pub finished_at: Option<u64>,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    #[ts(type = "number")]
    pub revision: u64,
    /// Last few detail lines; full log via `task_log`.
    pub details: Vec<String>,
}

/// Active + recent tasks returned by `task_list`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "types.ts")]
pub struct TaskBoard {
    pub active: Vec<TaskSnapshot>,
    pub recent: Vec<TaskSnapshot>,
    #[ts(type = "number")]
    pub generated_at: u64,
}

/// Declarative task definition supplied by the orchestration layer.
#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub kind: TaskKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub target: Option<String>,
    pub tag: Option<String>,
    pub origin: TaskOriginKind,
    pub origin_label: Option<String>,
    pub steps: Vec<TaskStepSpec>,
    pub cancellable: bool,
}

/// One step declaration (weight is relative, not a percentage).
#[derive(Debug, Clone)]
pub struct TaskStepSpec {
    pub id: String,
    pub label: String,
    pub weight: u32,
}

impl TaskSpec {
    pub fn new(kind: TaskKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            subtitle: None,
            target: None,
            tag: None,
            origin: TaskOriginKind::User,
            origin_label: None,
            steps: Vec::new(),
            cancellable: false,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    pub fn origin(mut self, origin: TaskOriginKind, label: Option<String>) -> Self {
        self.origin = origin;
        self.origin_label = label;
        self
    }

    pub fn cancellable(mut self, cancellable: bool) -> Self {
        self.cancellable = cancellable;
        self
    }

    pub fn step(mut self, id: impl Into<String>, label: impl Into<String>, weight: u32) -> Self {
        self.steps.push(TaskStepSpec {
            id: id.into(),
            label: label.into(),
            weight,
        });
        self
    }
}

/// Applies a batch progress sample to the task.
#[derive(Debug, Clone, Default)]
pub struct DownloadUpdate {
    pub completed_files: u64,
    pub total_files: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub current_file: Option<String>,
}

/// Task control errors surfaced to IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskControlError {
    NotFound,
    NotRunning,
    NotPausable,
    NotResumable,
}

impl fmt::Display for TaskControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "task not found"),
            Self::NotRunning => write!(f, "task is no longer running"),
            Self::NotPausable => write!(f, "task cannot be paused"),
            Self::NotResumable => write!(f, "task is not paused"),
        }
    }
}

impl std::error::Error for TaskControlError {}

// ── internal record ──

struct TaskStep {
    id: String,
    label: String,
    weight: u32,
    state: TaskStepState,
    percent: f64,
    completed: u64,
    total: u64,
    message: Option<String>,
}

impl TaskStep {
    fn fraction(&self) -> f64 {
        match self.state {
            TaskStepState::Done | TaskStepState::Skipped => 1.0,
            TaskStepState::Pending => 0.0,
            TaskStepState::Running | TaskStepState::Failed => {
                if self.percent > 0.0 {
                    (self.percent / 100.0).clamp(0.0, 1.0)
                } else if self.total > 0 {
                    (self.completed as f64 / self.total as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        }
    }

    fn snapshot(&self) -> TaskStepView {
        TaskStepView {
            id: self.id.clone(),
            label: self.label.clone(),
            state: self.state,
            weight: self.weight,
            percent: self.percent.clamp(0.0, 100.0),
            completed: self.completed,
            total: self.total,
            message: self.message.clone(),
        }
    }
}

struct TaskRecord {
    id: String,
    kind: TaskKind,
    state: TaskState,
    title: String,
    subtitle: Option<String>,
    target: Option<String>,
    tag: Option<String>,
    origin: TaskOriginKind,
    origin_label: Option<String>,
    steps: Vec<TaskStep>,
    message: String,
    download: Option<TaskDownloadView>,
    warnings: Vec<String>,
    error: Option<TaskErrorView>,
    result: Option<TaskResultView>,
    cancellable: bool,
    cancel: Arc<AtomicBool>,
    cancel_action: Option<Arc<dyn Fn() + Send + Sync>>,
    pause_action: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    details: VecDeque<String>,
    created_at: u64,
    started_at: u64,
    finished_at: Option<u64>,
    revision: u64,
    last_publish: Instant,
    last_sample: Option<(Instant, u64)>,
    speed_bps: f64,
}

impl TaskRecord {
    fn new(spec: TaskSpec, id: String) -> Self {
        let now = now_ms();
        let steps = spec
            .steps
            .into_iter()
            .map(|step| TaskStep {
                id: step.id,
                label: step.label,
                weight: step.weight,
                state: TaskStepState::Pending,
                percent: 0.0,
                completed: 0,
                total: 0,
                message: None,
            })
            .collect();
        Self {
            id,
            kind: spec.kind,
            state: TaskState::Running,
            title: spec.title,
            subtitle: spec.subtitle,
            target: spec.target,
            tag: spec.tag,
            origin: spec.origin,
            origin_label: spec.origin_label,
            steps,
            message: String::new(),
            download: None,
            warnings: Vec::new(),
            error: None,
            result: None,
            cancellable: spec.cancellable,
            cancel: Arc::new(AtomicBool::new(false)),
            cancel_action: None,
            pause_action: None,
            details: VecDeque::new(),
            created_at: now,
            started_at: now,
            finished_at: None,
            revision: 0,
            last_publish: Instant::now()
                .checked_sub(PUBLISH_INTERVAL)
                .unwrap_or_else(Instant::now),
            last_sample: None,
            speed_bps: 0.0,
        }
    }

    fn overall_percent(&self) -> f64 {
        if self.state == TaskState::Succeeded {
            return 100.0;
        }
        let total_weight: u32 = self.steps.iter().map(|step| step.weight).sum();
        if total_weight > 0 {
            let weighted: f64 = self
                .steps
                .iter()
                .map(|step| step.weight as f64 * step.fraction())
                .sum();
            return (weighted / total_weight as f64 * 100.0).clamp(0.0, 100.0);
        }
        if let Some(download) = &self.download {
            if download.total_bytes > 0 {
                return (download.downloaded_bytes as f64 / download.total_bytes as f64 * 100.0)
                    .clamp(0.0, 100.0);
            }
            if download.total_files > 0 {
                return (download.completed_files as f64 / download.total_files as f64 * 100.0)
                    .clamp(0.0, 100.0);
            }
        }
        0.0
    }

    fn current_step(&self) -> Option<&TaskStep> {
        self.steps
            .iter()
            .find(|step| step.state == TaskStepState::Running)
            .or_else(|| {
                self.steps
                    .iter()
                    .find(|step| step.state == TaskStepState::Failed)
            })
    }

    fn snapshot(&self) -> TaskSnapshot {
        let current = self.current_step();
        let message = if !self.message.is_empty() {
            self.message.clone()
        } else if let Some(step) = current {
            step.message.clone().unwrap_or_else(|| step.label.clone())
        } else {
            self.title.clone()
        };
        let elapsed_ms = self
            .finished_at
            .unwrap_or_else(now_ms)
            .saturating_sub(self.started_at);
        TaskSnapshot {
            id: self.id.clone(),
            kind: self.kind,
            state: self.state,
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            target: self.target.clone(),
            tag: self.tag.clone(),
            origin: self.origin,
            origin_label: self.origin_label.clone(),
            steps: self.steps.iter().map(TaskStep::snapshot).collect(),
            current_step: current.map(|step| step.id.clone()),
            current_step_label: current.map(|step| step.label.clone()),
            overall_percent: self.overall_percent(),
            message,
            download: self.download.clone(),
            warnings: self.warnings.clone(),
            error: self.error.clone(),
            result: self.result.clone(),
            capabilities: TaskCapabilities {
                can_pause: self.pause_action.is_some() && self.state == TaskState::Running,
                can_resume: self.pause_action.is_some() && self.state == TaskState::Paused,
                can_cancel: self.cancellable
                    && !self.state.is_terminal()
                    && self.state != TaskState::Cancelling,
                can_retry: false,
            },
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
            elapsed_ms,
            revision: self.revision,
            details: self
                .details
                .iter()
                .rev()
                .take(DETAIL_PREVIEW)
                .rev()
                .cloned()
                .collect(),
        }
    }
}

#[derive(Default)]
struct RegistryState {
    active: HashMap<String, TaskRecord>,
    history: VecDeque<TaskRecord>,
}

struct Inner {
    state: RwLock<RegistryState>,
    tx: broadcast::Sender<TaskSnapshot>,
}

fn write_state(inner: &Inner) -> std::sync::RwLockWriteGuard<'_, RegistryState> {
    match inner.state.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn publish_locked(state: &mut RegistryState, id: &str, force: bool) -> Option<TaskSnapshot> {
    let record = state.active.get_mut(id)?;
    if !force && record.last_publish.elapsed() < PUBLISH_INTERVAL {
        return None;
    }
    record.last_publish = Instant::now();
    record.revision = record.revision.saturating_add(1);
    Some(record.snapshot())
}

/// Central registry. Cloning is cheap (one `Arc`).
#[derive(Clone)]
pub struct TaskRegistry {
    inner: Arc<Inner>,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRegistry {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                state: RwLock::new(RegistryState::default()),
                tx,
            }),
        }
    }

    /// Create a task and return its handle. The initial snapshot is broadcast
    /// before the caller performs any work, so the UI never has to invent a
    /// pending row or re-key a batch id later.
    pub fn create(&self, spec: TaskSpec) -> TaskHandle {
        let id = uuid_like();
        let record = TaskRecord::new(spec, id.clone());
        let cancel = record.cancel.clone();
        let snapshot = {
            let mut state = write_state(&self.inner);
            let mut record = record;
            record.revision = 1;
            let snapshot = record.snapshot();
            state.active.insert(id.clone(), record);
            snapshot
        };
        let _ = self.inner.tx.send(snapshot);
        TaskHandle {
            inner: self.inner.clone(),
            id,
            cancel,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskSnapshot> {
        self.inner.tx.subscribe()
    }

    pub fn board(&self) -> TaskBoard {
        let state = match self.inner.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut active: Vec<TaskSnapshot> =
            state.active.values().map(TaskRecord::snapshot).collect();
        active.sort_by_key(|task| task.created_at);
        TaskBoard {
            active,
            recent: state.history.iter().map(TaskRecord::snapshot).collect(),
            generated_at: now_ms(),
        }
    }

    pub fn snapshot(&self, id: &str) -> Option<TaskSnapshot> {
        let state = match self.inner.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .active
            .get(id)
            .or_else(|| state.history.iter().find(|record| record.id == id))
            .map(TaskRecord::snapshot)
    }

    pub fn log(&self, id: &str) -> Vec<String> {
        let state = match self.inner.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .active
            .get(id)
            .or_else(|| state.history.iter().find(|record| record.id == id))
            .map(|record| record.details.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear_history(&self) {
        let mut state = write_state(&self.inner);
        state.history.clear();
    }

    pub fn request_cancel(&self, id: &str) -> Result<(), TaskControlError> {
        let (action, snapshot) = {
            let mut state = write_state(&self.inner);
            let record = state.active.get_mut(id).ok_or(TaskControlError::NotFound)?;
            if record.state.is_terminal() {
                return Err(TaskControlError::NotRunning);
            }
            record.cancel.store(true, Ordering::Relaxed);
            record.state = TaskState::Cancelling;
            record.message = "正在取消…".to_string();
            let action = record.cancel_action.clone();
            let snapshot = publish_locked(&mut state, id, true);
            (action, snapshot)
        };
        if let Some(action) = action {
            action();
        }
        if let Some(snapshot) = snapshot {
            let _ = self.inner.tx.send(snapshot);
        }
        Ok(())
    }

    pub fn request_pause(&self, id: &str) -> Result<(), TaskControlError> {
        self.request_pause_state(id, true)
    }

    pub fn request_resume(&self, id: &str) -> Result<(), TaskControlError> {
        self.request_pause_state(id, false)
    }

    fn request_pause_state(&self, id: &str, pause: bool) -> Result<(), TaskControlError> {
        let (action, snapshot) = {
            let mut state = write_state(&self.inner);
            let record = state.active.get_mut(id).ok_or(TaskControlError::NotFound)?;
            if record.state.is_terminal() || record.state == TaskState::Cancelling {
                return Err(TaskControlError::NotRunning);
            }
            let action = record.pause_action.clone().ok_or(if pause {
                TaskControlError::NotPausable
            } else {
                TaskControlError::NotResumable
            })?;
            if pause {
                if record.state != TaskState::Running {
                    return Err(TaskControlError::NotPausable);
                }
                record.state = TaskState::Paused;
                record.message = "已暂停".to_string();
                record.speed_bps = 0.0;
                record.last_sample = None;
            } else {
                if record.state != TaskState::Paused {
                    return Err(TaskControlError::NotResumable);
                }
                record.state = TaskState::Running;
                record.message.clear();
            }
            let snapshot = publish_locked(&mut state, id, true);
            (Some(action), snapshot)
        };
        if let Some(action) = action {
            action(pause);
        }
        if let Some(snapshot) = snapshot {
            let _ = self.inner.tx.send(snapshot);
        }
        Ok(())
    }
}

/// Handle used by the orchestration worker to drive one task.
#[derive(Clone)]
pub struct TaskHandle {
    inner: Arc<Inner>,
    id: String,
    cancel: Arc<AtomicBool>,
}

impl TaskHandle {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Cooperative cancellation flag. Workers poll this between stages and
    /// pass it to long-running helpers (modpack core expects exactly this).
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Whether the user paused this task (registry state, not the batch).
    pub fn is_paused(&self) -> bool {
        let state = match self.inner.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .active
            .get(&self.id)
            .map(|record| record.state == TaskState::Paused)
            .unwrap_or(false)
    }

    pub fn set_cancel_action<F>(&self, action: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.apply(true, |record| {
            record.cancel_action = Some(Arc::new(action));
        });
    }

    pub fn set_pause_action<F>(&self, action: F)
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.apply(true, |record| {
            record.pause_action = Some(Arc::new(action));
        });
    }

    /// Drop pause capability once the task leaves its download step.
    pub fn clear_pause_action(&self) {
        self.apply(true, |record| {
            record.pause_action = None;
        });
    }

    pub fn begin_step(&self, id: impl AsRef<str>) {
        let id = id.as_ref();
        self.apply(true, |record| {
            let target = record.steps.iter().position(|step| step.id == id);
            if let Some(index) = target {
                for (position, step) in record.steps.iter_mut().enumerate() {
                    if position == index {
                        if step.state == TaskStepState::Pending {
                            step.state = TaskStepState::Running;
                        }
                    } else if step.state == TaskStepState::Running {
                        step.state = TaskStepState::Done;
                        step.percent = 100.0;
                    }
                }
            }
        });
    }

    pub fn set_step_progress(
        &self,
        id: impl AsRef<str>,
        completed: u64,
        total: u64,
        percent: Option<f64>,
        message: Option<String>,
    ) {
        let id = id.as_ref();
        self.apply(false, |record| {
            if let Some(step) = record.steps.iter_mut().find(|step| step.id == id) {
                if step.state == TaskStepState::Pending {
                    step.state = TaskStepState::Running;
                }
                step.completed = completed;
                step.total = total;
                if let Some(percent) = percent {
                    step.percent = percent.clamp(0.0, 100.0);
                } else if total > 0 {
                    step.percent = (completed as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                }
                if message.is_some() {
                    step.message = message;
                }
            }
        });
    }

    pub fn set_step_percent(&self, id: impl AsRef<str>, percent: f64) {
        self.set_step_progress(id, 0, 0, Some(percent), None);
    }

    pub fn complete_step(&self, id: impl AsRef<str>) {
        let id = id.as_ref();
        self.apply(true, |record| {
            if let Some(step) = record.steps.iter_mut().find(|step| step.id == id) {
                step.state = TaskStepState::Done;
                step.percent = 100.0;
            }
        });
    }

    pub fn skip_step(&self, id: impl AsRef<str>) {
        let id = id.as_ref();
        self.apply(true, |record| {
            if let Some(step) = record.steps.iter_mut().find(|step| step.id == id) {
                step.state = TaskStepState::Skipped;
                step.percent = 100.0;
            }
        });
    }

    pub fn fail_step(&self, id: impl AsRef<str>, message: impl Into<String>) {
        let id = id.as_ref();
        let message = message.into();
        self.apply(true, |record| {
            if let Some(step) = record.steps.iter_mut().find(|step| step.id == id) {
                step.state = TaskStepState::Failed;
                step.message = Some(message);
            }
        });
    }

    /// Feed a download batch sample into a step.
    pub fn download(&self, step_id: impl AsRef<str>, update: DownloadUpdate) {
        let step_id = step_id.as_ref();
        self.apply(false, |record| {
            let now = Instant::now();
            if let Some((last_at, last_bytes)) = record.last_sample {
                let elapsed = now.duration_since(last_at).as_secs_f64();
                if elapsed >= 0.1 {
                    if update.downloaded_bytes >= last_bytes {
                        let instant = (update.downloaded_bytes - last_bytes) as f64 / elapsed;
                        record.speed_bps = if record.speed_bps <= 0.0 {
                            instant
                        } else {
                            record.speed_bps * 0.7 + instant * 0.3
                        };
                    } else {
                        // A new batch restarted the counters.
                        record.speed_bps = 0.0;
                    }
                    record.last_sample = Some((now, update.downloaded_bytes));
                }
            } else {
                record.last_sample = Some((now, update.downloaded_bytes));
            }

            let eta_seconds =
                if record.speed_bps > 1.0 && update.total_bytes > update.downloaded_bytes {
                    Some(
                        ((update.total_bytes - update.downloaded_bytes) as f64 / record.speed_bps)
                            .ceil() as u64,
                    )
                } else {
                    None
                };
            record.download = Some(TaskDownloadView {
                completed_files: update.completed_files,
                total_files: update.total_files,
                downloaded_bytes: update.downloaded_bytes,
                total_bytes: update.total_bytes,
                speed_bps: record.speed_bps,
                eta_seconds,
                current_file: update.current_file.clone(),
            });

            if let Some(step) = record.steps.iter_mut().find(|step| step.id == step_id) {
                if step.state == TaskStepState::Pending {
                    step.state = TaskStepState::Running;
                }
                step.completed = update.completed_files;
                step.total = update.total_files;
                step.percent = if update.total_bytes > 0 {
                    (update.downloaded_bytes as f64 / update.total_bytes as f64 * 100.0)
                        .clamp(0.0, 100.0)
                } else if update.total_files > 0 {
                    (update.completed_files as f64 / update.total_files as f64 * 100.0)
                        .clamp(0.0, 100.0)
                } else {
                    step.percent
                };
                if update.current_file.is_some() {
                    step.message = update.current_file.clone();
                }
            }
        });
    }

    /// Clear stale batch counters when a task moves on to a non-download step.
    pub fn clear_download(&self) {
        self.apply(false, |record| {
            record.download = None;
            record.speed_bps = 0.0;
            record.last_sample = None;
        });
    }

    pub fn message(&self, message: impl Into<String>) {
        let message = message.into();
        self.apply(false, |record| {
            record.message = message;
        });
    }

    pub fn detail(&self, line: impl Into<String>) {
        let line = line.into();
        self.apply(false, |record| {
            if line.trim().is_empty() {
                return;
            }
            record.details.push_back(line);
            while record.details.len() > DETAIL_CAPACITY {
                record.details.pop_front();
            }
        });
    }

    pub fn warn(&self, warning: impl Into<String>) {
        let warning = warning.into();
        self.apply(true, |record| {
            record.warnings.push(warning);
        });
    }

    pub fn succeed(&self, result: TaskResultView) {
        self.finish(TaskState::Succeeded, None, Some(result));
    }

    pub fn fail(&self, error: TaskErrorView) {
        self.finish(TaskState::Failed, Some(error), None);
    }

    pub fn cancelled(&self) {
        self.message("任务已取消");
        self.finish(
            TaskState::Cancelled,
            Some(TaskErrorView {
                message: "任务已取消".to_string(),
                code: Some("cancelled".to_string()),
                retryable: false,
            }),
            None,
        );
    }

    fn finish(
        &self,
        state: TaskState,
        error: Option<TaskErrorView>,
        result: Option<TaskResultView>,
    ) {
        let snapshot = {
            let mut state_guard = write_state(&self.inner);
            let Some(mut record) = state_guard.active.remove(&self.id) else {
                return;
            };
            record.state = state;
            record.cancel_action = None;
            record.pause_action = None;
            record.error = error;
            record.result = result;
            record.finished_at = Some(now_ms());
            record.last_publish = Instant::now();
            record.revision = record.revision.saturating_add(1);
            if state == TaskState::Succeeded {
                for step in &mut record.steps {
                    if step.state == TaskStepState::Pending || step.state == TaskStepState::Running
                    {
                        step.state = TaskStepState::Done;
                        step.percent = 100.0;
                    }
                }
                record.download = None;
            }
            let snapshot = record.snapshot();
            state_guard.history.push_front(record);
            while state_guard.history.len() > HISTORY_CAPACITY {
                state_guard.history.pop_back();
            }
            snapshot
        };
        let _ = self.inner.tx.send(snapshot);
    }

    fn apply<F>(&self, force: bool, mutate: F)
    where
        F: FnOnce(&mut TaskRecord),
    {
        let snapshot = {
            let mut state = write_state(&self.inner);
            let Some(record) = state.active.get_mut(&self.id) else {
                return;
            };
            mutate(record);
            publish_locked(&mut state, &self.id, force)
        };
        if let Some(snapshot) = snapshot {
            let _ = self.inner.tx.send(snapshot);
        }
    }
}

/// Small dependency-free id generator (the task id is opaque to the UI).
fn uuid_like() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{:x}", now_ms(), std::process::id(), seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> TaskSpec {
        TaskSpec::new(TaskKind::InstanceInstall, "安装 1.20.4")
            .target("dir/instance")
            .step("game", "游戏文件", 1)
            .step("loader", "加载器", 3)
    }

    #[test]
    fn overall_progress_is_weighted_across_steps() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec());
        task.begin_step("game");
        task.set_step_progress("game", 50, 100, Some(50.0), None);
        let snapshot = registry.snapshot(task.id()).expect("task");
        // game weight 1 at 50% + loader weight 3 at 0% → 12.5%.
        assert!((snapshot.overall_percent - 12.5).abs() < 0.01);
    }

    #[test]
    fn success_completes_steps_and_moves_to_history() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec());
        task.begin_step("game");
        task.succeed(TaskResultView {
            key: Some("dir/instance".into()),
            ..TaskResultView::default()
        });
        assert!(registry.board().active.is_empty());
        let recent = registry.board().recent;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].state, TaskState::Succeeded);
        assert!(recent[0]
            .steps
            .iter()
            .all(|step| step.state == TaskStepState::Done));
        assert!((recent[0].overall_percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pause_requires_a_registered_action() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec());
        assert_eq!(
            registry.request_pause(task.id()),
            Err(TaskControlError::NotPausable)
        );
        let seen = Arc::new(AtomicBool::new(false));
        let flag = seen.clone();
        task.set_pause_action(move |paused| flag.store(paused, Ordering::Relaxed));
        registry.request_pause(task.id()).expect("pause");
        assert!(seen.load(Ordering::Relaxed));
        assert_eq!(
            registry.snapshot(task.id()).expect("task").state,
            TaskState::Paused
        );
        registry.request_resume(task.id()).expect("resume");
        assert!(!seen.load(Ordering::Relaxed));
    }

    #[test]
    fn cancel_sets_flag_and_cancelling_state() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec().cancellable(true));
        task.set_cancel_action(|| {});
        registry.request_cancel(task.id()).expect("cancel");
        assert!(task.cancel_requested());
        let snapshot = registry.snapshot(task.id()).expect("task");
        assert_eq!(snapshot.state, TaskState::Cancelling);
        assert!(!snapshot.capabilities.can_cancel);
    }

    #[test]
    fn detail_log_is_capped() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec());
        for index in 0..(DETAIL_CAPACITY + 20) {
            task.detail(format!("line {index}"));
        }
        let log = registry.log(task.id());
        assert_eq!(log.len(), DETAIL_CAPACITY);
        assert_eq!(log.last().map(String::as_str), Some("line 259"));
        let preview = registry.snapshot(task.id()).expect("task").details;
        assert_eq!(preview.len(), DETAIL_PREVIEW);
    }

    #[test]
    fn terminal_tasks_ignore_late_progress() {
        let registry = TaskRegistry::new();
        let task = registry.create(spec());
        task.succeed(TaskResultView::default());
        task.set_step_percent("game", 90.0);
        let recent = registry.board().recent;
        assert_eq!(recent.len(), 1);
        assert!((recent[0].overall_percent - 100.0).abs() < f64::EPSILON);
    }
}
