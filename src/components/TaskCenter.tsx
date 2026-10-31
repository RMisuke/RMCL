/**
 * Task center (ui-design 10, rebuilt around the unified task model).
 *
 * A task is one visible install/download transaction: weighted step timeline,
 * live file/byte counters, speed, ETA, detail log, and pause/resume/cancel.
 * Download batches and loader stages are implementation details inside it.
 */
import type { JSX } from "solid-js";
import {
  Show,
  For,
  createEffect,
  createMemo,
  createSignal,
  onMount,
  onCleanup,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import { Icon, type IconName } from "./icons";
import { RmButton, RmIconButton, RmProgress, RmTag } from "./rm/controls";
import { tasksStore } from "../stores/tasks";
import { launchStore } from "../stores/launch";
import { instancesStore } from "../stores/instances";
import { mcpStore } from "../stores/mcp";
import { uiStore, toast, confirmDanger } from "../stores/ui";
import { t } from "../i18n";
import { formatBytes, groupDigits, relativeTime } from "../lib/format";
import type {
  McpConfirmationView,
  TaskKind,
  TaskSnapshot,
  TaskState,
} from "../ipc/types";

function stateLabel(state: TaskState): string {
  return t(`tasks.state.${state}`);
}

function kindIcon(kind: TaskKind): IconName {
  switch (kind) {
    case "instance-install":
      return "package";
    case "modpack-install":
      return "layers";
    case "modloader-install":
      return "wrench";
    case "mod-install":
      return "extension";
    case "dependency-install":
      return "shield";
    case "instance-repair":
      return "refresh";
    default:
      return "download";
  }
}

function kindLabel(kind: TaskKind): string {
  return t(`tasks.kind.${kind}`);
}

function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes >= 60) {
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function formatEta(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds)) return null;
  return formatDuration(seconds * 1000);
}

// ── Rail indicator ──

export function TaskIndicator(props?: { compact?: boolean }): JSX.Element {
  const activeCount = createMemo(() => tasksStore.activeCount() + launchStore.running().size);
  const pendingConfirmations = createMemo(() => mcpStore.confirmations().length);
  const aggregate = createMemo(() => tasksStore.aggregate());

  return (
    <button
      class={`rail-task-indicator${props?.compact ? " is-compact" : ""}`}
      onClick={() => uiStore.toggleTaskCenter()}
      aria-label={t("tasks.title")}
      title={t("tasks.title")}
    >
      <span class="rail-task-ring">
        <svg viewBox="0 0 24 24" width="20" height="20">
          <circle cx="12" cy="12" r="9" fill="none" stroke="var(--rmccc-border)" stroke-width="3" />
          <Show when={tasksStore.activeCount() > 0}>
            <circle
              cx="12"
              cy="12"
              r="9"
              fill="none"
              stroke="var(--rmccc-accent)"
              stroke-width="3"
              stroke-dasharray={`${(aggregate().percent / 100) * 56.5} 56.5`}
              stroke-dashoffset="14.1"
              stroke-linecap="round"
            />
          </Show>
        </svg>
        <Show when={pendingConfirmations() > 0}>
          <span class="rail-task-alert" />
        </Show>
      </span>
      <Show when={!props?.compact}>
        <span class="rail-task-label">{t("tasks.title")}</span>
      </Show>
      <Show when={activeCount() > 0}>
        <span class="rail-task-count">{activeCount()}</span>
      </Show>
    </button>
  );
}

// ── Panel ──

export function TaskCenterPanel(): JSX.Element {
  const open = () => uiStore.taskCenterOpen();
  const navigate = useNavigate();
  const active = createMemo(() => tasksStore.activeList());
  const games = () => [...launchStore.running().values()];
  const recent = () => tasksStore.recent();
  const aggregate = createMemo(() => tasksStore.aggregate());
  const expanded = createSignal<Record<string, boolean>>({});
  const [expandedMap, setExpandedMap] = expanded;
  const toggleDetails = (taskId: string) =>
    setExpandedMap((map) => ({ ...map, [taskId]: !map[taskId] }));

  const hasContent = createMemo(
    () =>
      open() &&
      (active().length + games().length + recent().length + mcpStore.confirmations().length > 0)
  );

  function openResult(task: TaskSnapshot): void {
    uiStore.setTaskCenterOpen(false);
    if (task.result?.manualDirectory) {
      void instancesStore.selectDirectory(task.result.manualDirectory);
    } else if (task.result?.key) {
      void instancesStore.selectInstance(task.result.key);
      navigate(`/instances/${encodeURIComponent(task.result.key)}`);
    }
  }

  return (
    <Show when={open()}>
      <div class="task-center-overlay" onClick={() => uiStore.setTaskCenterOpen(false)}>
        <div class="task-center" onClick={(e) => e.stopPropagation()}>
          <header class="task-center-head">
            <div class="task-center-title">
              <h3>{t("tasks.title")}</h3>
              <span class="task-center-summary">
                {t("tasks.activeSummary", { n: String(active().length) })}
                <Show when={aggregate().speed > 0}>
                  {" · "}
                  {formatBytes(aggregate().speed)}/s
                </Show>
              </span>
            </div>
            <RmIconButton icon="x" label={t("common.close")} onClick={() => uiStore.setTaskCenterOpen(false)} />
          </header>

          <div class="task-center-body">
            <Show when={mcpStore.confirmations().length > 0}>
              <div class="task-section">
                <div class="task-section-title is-danger">{t("tasks.confirm.section")}</div>
                <For each={mcpStore.confirmations()}>
                  {(confirmation) => <ConfirmationCard confirmation={confirmation} />}
                </For>
              </div>
            </Show>

            <Show when={active().length > 0}>
              <div class="task-section">
                <div class="task-section-title">
                  <span>{t("tasks.now")}</span>
                  <span class="task-section-meta">{t("tasks.runningCount", { n: String(active().length) })}</span>
                </div>
                <For each={active()}>
                  {(task) => (
                    <TaskCard
                      task={task}
                      expanded={!!expandedMap()[task.id]}
                      onToggle={() => toggleDetails(task.id)}
                    />
                  )}
                </For>
              </div>
            </Show>

            <Show when={games().length > 0}>
              <div class="task-section">
                <div class="task-section-title">{t("tasks.sectionGames")}</div>
                <For each={games()}>
                  {(game) => (
                    <div class="task-row">
                      <div class="task-row-main">
                        <div class="task-row-title">
                          <span class="dot-running" /> {profileName(game.profileId)}
                        </div>
                        <div class="task-row-subtitle">
                          {t("tasks.game.startedAt", {
                            time: relativeTime(new Date(game.startedAt).toISOString()),
                          })}
                        </div>
                      </div>
                      <div class="task-row-actions">
                        <RmButton size="sm" icon="terminal" onClick={() => uiStore.openLogDrawer(game.profileId)}>
                          {t("tasks.game.log")}
                        </RmButton>
                        <RmButton
                          size="sm"
                          variant="danger"
                          icon="square"
                          onClick={() => void launchStore.stop(game.profileId)}
                        >
                          {t("tasks.game.stop")}
                        </RmButton>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <Show when={recent().length > 0}>
              <div class="task-section task-recent">
                <div class="task-section-title">
                  <span>{t("tasks.recent")}</span>
                  <button class="task-link-button" onClick={() => void tasksStore.clearHistory()}>
                    {t("tasks.clearRecent")}
                  </button>
                </div>
                <For each={recent().slice(0, 8)}>
                  {(task) => <RecentRow task={task} onOpen={() => openResult(task)} />}
                </For>
              </div>
            </Show>

            <Show when={!hasContent()}>
              <div class="task-center-empty">
                <Icon name="check" size={22} />
                <span>{t("tasks.empty")}</span>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}

function profileName(profileId: string): string {
  return instancesStore.items()?.find((p) => p.summary.key === profileId)?.summary.display_name ?? profileId;
}

// ── Task card ──

function TaskCard(props: {
  task: TaskSnapshot;
  expanded: boolean;
  onToggle: () => void;
}): JSX.Element {
  const task = () => props.task;
  const percent = () => Math.min(100, Math.max(0, task().overallPercent));
  const download = () => task().download;
  const stepIndex = createMemo(() => {
    const index = task().steps.findIndex((step) => step.id === task().currentStep);
    return index >= 0 ? index + 1 : task().steps.filter((step) => step.state === "done").length;
  });
  const indeterminate = () =>
    task().state === "running" && percent() <= 0 && !download();
  const [log, setLog] = createSignal<string[] | null>(null);
  const [logFor, setLogFor] = createSignal("");

  async function cancel(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("tasks.cancelConfirm.title"),
      body: t("tasks.cancelConfirm.body"),
      confirmLabel: t("tasks.download.cancel"),
    });
    if (allowed) await tasksStore.cancel(task().id);
  }

  createEffect(() => {
    if (!props.expanded) return;
    // The snapshot carries a preview only; refresh the full log when the
    // task revision changes (at most the registry's 100 ms publish cadence).
    void task().revision;
    const id = task().id;
    if (logFor() !== `${id}:${task().revision}`) {
      setLogFor(`${id}:${task().revision}`);
      void tasksStore.log(id).then(setLog);
    }
  });

  return (
    <article class={`task-card is-${task().state}`}>
      <div class="task-card-head">
        <span class={`task-kind-icon is-${task().kind}`}>
          <Icon name={kindIcon(task().kind)} size={16} />
        </span>
        <div class="task-card-title">
          <div class="task-card-title-row">
            <strong>{task().title}</strong>
            <Show when={task().origin !== "user"}>
              <span class="task-origin-badge" title={task().originLabel ?? ""}>
                <Icon name={task().origin === "agent" ? "bot" : "extension"} size={11} />
                {task().origin === "agent" ? "Agent" : task().originLabel ?? "Plugin"}
              </span>
            </Show>
          </div>
          <div class="task-card-subtitle">
            {kindLabel(task().kind)}
            <Show when={task().subtitle}> · {task().subtitle}</Show>
          </div>
        </div>
        <span class={`task-state-chip is-${task().state}`}>{stateLabel(task().state)}</span>
      </div>

      <div class="task-card-progress">
        <div class="task-progress-meta">
          <span class="task-progress-message">
            <Show when={task().currentStepLabel} fallback={task().message}>
              <strong>{task().currentStepLabel}</strong>
              <Show when={task().message}> · {task().message}</Show>
            </Show>
          </span>
          <strong class="task-progress-percent">{Math.round(percent())}%</strong>
        </div>
        <RmProgress value={percent()} determinate={!indeterminate()} />
        <div class="task-step-rail" aria-hidden="true">
          <For each={task().steps}>
            {(step, index) => (
              <>
                <Show when={index() > 0}>
                  <span class={`task-step-connector is-${step.state}`} />
                </Show>
                <span
                  class={`task-step-dot is-${step.state}`}
                  title={`${step.label}${step.message ? ` · ${step.message}` : ""}`}
                />
              </>
            )}
          </For>
        </div>
      </div>

      <div class="task-card-stats">
        <span class="task-stat">
          <Icon name="list" size={12} />
          {t("tasks.stepProgress", {
            current: String(Math.max(1, stepIndex())),
            total: String(task().steps.length),
          })}
        </span>
        <Show when={download()}>
          {(data) => (
            <>
              <span class="task-stat">
                <Icon name="file" size={12} />
                {groupDigits(data().completedFiles)}/{groupDigits(data().totalFiles)}
              </span>
              <span class="task-stat">
                <Icon name="download" size={12} />
                {formatBytes(data().downloadedBytes)}/{formatBytes(data().totalBytes)}
              </span>
              <Show when={data().speedBps > 0}>
                <span class="task-stat is-accent">{formatBytes(data().speedBps)}/s</span>
              </Show>
              <Show when={formatEta(data().etaSeconds)}>
                {(eta) => (
                  <span class="task-stat">
                    <Icon name="clock" size={12} />
                    {t("tasks.eta", { time: eta() })}
                  </span>
                )}
              </Show>
            </>
          )}
        </Show>
        <Show when={task().elapsedMs > 0}>
          <span class="task-stat task-stat-spacer">{formatDuration(task().elapsedMs)}</span>
        </Show>
      </div>

      <Show when={download()?.currentFile}>
        {(file) => <div class="task-current-file" title={file()}>{file()}</div>}
      </Show>

      <Show when={task().error}>
        {(error) => <div class="task-error">{error().message}</div>}
      </Show>

      <div class="task-card-actions">
        <button class="task-link-button" onClick={() => props.onToggle()}>
          <Icon name={props.expanded ? "chevron-up" : "chevron-down"} size={12} />
          {props.expanded ? t("tasks.hideDetails") : t("tasks.showDetails")}
        </button>
        <span class="task-action-spacer" />
        <Show when={task().capabilities.canPause}>
          <RmButton
            size="sm"
            variant="ghost"
            icon="pause"
            onClick={() => void tasksStore.pause(task().id)}
          >
            {t("tasks.download.pause")}
          </RmButton>
        </Show>
        <Show when={task().capabilities.canResume}>
          <RmButton
            size="sm"
            variant="ghost"
            icon="play"
            onClick={() => void tasksStore.resume(task().id)}
          >
            {t("tasks.download.resume")}
          </RmButton>
        </Show>
        <Show when={task().capabilities.canCancel}>
          <RmButton size="sm" variant="ghost" icon="x" onClick={() => void cancel()}>
            {t("tasks.download.cancel")}
          </RmButton>
        </Show>
      </div>

      <Show when={props.expanded}>
        <div class="task-card-detail">
          <div class="task-step-list">
            <For each={task().steps}>
              {(step) => (
                <div class={`task-step-row is-${step.state}`}>
                  <span class="task-step-bullet">
                    <Show when={step.state === "done"} fallback={<span />}>
                      <Icon name="check" size={9} />
                    </Show>
                  </span>
                  <span class="task-step-label">{step.label}</span>
                  <span class="task-step-state">
                    {step.state === "running"
                      ? `${Math.round(step.percent)}%`
                      : step.state === "failed"
                        ? t("tasks.step.failed")
                        : step.state === "done"
                          ? t("tasks.step.done")
                          : step.state === "skipped"
                            ? t("tasks.step.skipped")
                            : ""}
                  </span>
                  <Show when={step.message}>
                    <span class="task-step-message">{step.message}</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
          <Show when={log()?.length}>
            <div class="task-detail-log">
              <For each={log()!.slice(-40)}>
                {(line) => <div class="task-detail-line">{line}</div>}
              </For>
            </div>
          </Show>
          <Show when={task().warnings.length > 0}>
            <div class="task-warnings">
              <For each={task().warnings}>{(warning) => <div>{warning}</div>}</For>
            </div>
          </Show>
        </div>
      </Show>
    </article>
  );
}

function RecentRow(props: { task: TaskSnapshot; onOpen: () => void }): JSX.Element {
  const ok = () => props.task.state === "succeeded";
  return (
    <div class={`task-recent-row ${ok() ? "is-ok" : props.task.state === "cancelled" ? "" : "is-failed"}`}>
      <Icon name={ok() ? "check" : props.task.state === "cancelled" ? "x" : "alert"} size={14} />
      <div class="task-recent-copy">
        <span class="task-recent-label">{props.task.title}</span>
        <span class="task-recent-detail">{props.task.error?.message ?? props.task.message}</span>
      </div>
      <span class="task-recent-time">{formatDuration(props.task.elapsedMs)}</span>
      <Show when={ok() && (props.task.result?.key || props.task.result?.manualDirectory)}>
        <button class="task-link-button" onClick={props.onOpen}>
          {t("tasks.viewResult")}
        </button>
      </Show>
    </div>
  );
}

// ── MCP destructive-tool confirmation ──

function ConfirmationCard(props: { confirmation: McpConfirmationView }): JSX.Element {
  const [countdown, setCountdown] = createSignal(120);
  onMount(() => {
    const created = Date.parse(props.confirmation.createdAt);
    const tick = setInterval(() => {
      const elapsed = Math.floor((Date.now() - created) / 1000);
      setCountdown(Math.max(0, 120 - elapsed));
    }, 1000);
    onCleanup(() => clearInterval(tick));
  });

  const impact = createMemo(() => describeConfirmation(props.confirmation));

  return (
    <div class="confirm-card" role="alertdialog">
      <div class="confirm-card-head">
        <Icon name="alert" size={16} />
        <span>{t("tasks.confirm.title")}</span>
        <span class="confirm-card-timer">{countdown()}s</span>
      </div>
      <div class="confirm-card-body">
        <div class="confirm-card-tool">{props.confirmation.tool}</div>
        <div class="confirm-card-impact">{impact()}</div>
      </div>
      <div class="confirm-card-actions">
        <RmButton variant="danger" icon="check" onClick={() => void mcpStore.resolve(props.confirmation.id, true)}>
          {t("tasks.confirm.allow")}
        </RmButton>
        <RmButton icon="x" onClick={() => void mcpStore.resolve(props.confirmation.id, false)}>
          {t("tasks.confirm.deny")}
        </RmButton>
      </div>
    </div>
  );
}

function describeConfirmation(confirmation: McpConfirmationView): string {
  const payload = confirmation.payload as Record<string, unknown>;
  switch (confirmation.tool) {
    case "profiles.delete":
      return t("tasks.confirm.impact.profileDelete", { id: String(payload.profile_id ?? "") });
    case "instances.remove":
      return t("tasks.confirm.impact.instanceRemove", { id: String(payload.version_id ?? "") });
    case "mods.remove":
      return t("tasks.confirm.impact.modRemove", { file: String(payload.mod_filename ?? "") });
    case "launch.kill":
      return t("tasks.confirm.impact.launchKill");
    case "settings.set":
      return t("tasks.confirm.impact.settingsSet");
    default:
      return JSON.stringify(payload);
  }
}
