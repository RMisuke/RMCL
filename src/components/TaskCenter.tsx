/**
 * Task center (ui-design 10): Rail activity indicator + non-modal popover
 * panel. Pending MCP confirmations render on top; then downloads, loader
 * installs, running games, and the recent-outcomes fold.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal } from "solid-js";
import { Icon } from "./icons";
import { RmButton, RmIconButton, RmProgress, RmTag } from "./rm/controls";
import { downloadsStore } from "../stores/downloads";
import { downloads } from "../ipc/api";
import { modloadersStore } from "../stores/modloaders";
import { launchStore } from "../stores/launch";
import { profilesStore } from "../stores/profiles";
import { mcpStore } from "../stores/mcp";
import { uiStore, toast, confirmDanger } from "../stores/ui";
import { t } from "../i18n";
import { formatBytes, groupDigits, relativeTime } from "../lib/format";
import { loaderLabel } from "../lib/loaders";
import type { McpConfirmationView } from "../ipc/types";

// ── Rail indicator ──

export function TaskIndicator(props?: { compact?: boolean }): JSX.Element {
  const activeCount = createMemo(() => {
    const downloadsCount = downloadsStore.batches().size;
    const loaders = modloadersStore.installs().size;
    const games = launchStore.running().size;
    return downloadsCount + loaders + games;
  });
  const pendingConfirmations = createMemo(() => mcpStore.confirmations().length);
  const bytes = createMemo(() => downloadsStore.totalBytesProgress());
  const percent = createMemo(() => {
    const { downloaded, total } = bytes();
    return total > 0 ? (downloaded / total) * 100 : 0;
  });

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
          <Show when={activeCount() > 0 && percent() > 0}>
            <circle
              cx="12"
              cy="12"
              r="9"
              fill="none"
              stroke="var(--rmccc-accent)"
              stroke-width="3"
              stroke-dasharray={`${(percent() / 100) * 56.5} 56.5`}
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

// ── Popover panel ──

export function TaskCenterPanel(): JSX.Element {
  const open = () => uiStore.taskCenterOpen();
  const batches = () => [...downloadsStore.batches().values()];
  const loaders = () => [...modloadersStore.installs().values()];
  const games = () => [...launchStore.running().values()];
  const hasContent = createMemo(
    () => open() && (batches().length + loaders().length + games().length + mcpStore.confirmations().length > 0)
  );

  return (
    <Show when={open()}>
      <div class="task-center-overlay" onClick={() => uiStore.setTaskCenterOpen(false)}>
        <div class="task-center" onClick={(e) => e.stopPropagation()}>
          <header class="task-center-head">
            <h3>{t("tasks.title")}</h3>
            <RmIconButton icon="x" label={t("common.close")} onClick={() => uiStore.setTaskCenterOpen(false)} />
          </header>

          <div class="task-center-body">
            <Show when={mcpStore.confirmations().length > 0}>
              <div class="task-section-title is-danger">{t("tasks.confirm.section")}</div>
              <For each={mcpStore.confirmations()}>
                {(confirmation) => <ConfirmationCard confirmation={confirmation} />}
              </For>
            </Show>

            <Show when={batches().length > 0}>
              <div class="task-section-title">{t("tasks.sectionDownloads")}</div>
              <For each={batches()}>{(batch) => <DownloadRow batch={batch} />}</For>
            </Show>

            <Show when={loaders().length > 0}>
              <div class="task-section-title">{t("tasks.sectionLoaders")}</div>
              <For each={loaders()}>
                {(install) => (
                  <div class="task-row">
                    <div class="task-row-main">
                      <div class="task-row-title">
                        {loaderLabel(install.loaderType)} → {profileName(install.profileId)}
                      </div>
                      <div class="task-row-subtitle">{install.message}</div>
                    </div>
                    <RmProgress value={install.percent} />
                  </div>
                )}
              </For>
            </Show>

            <Show when={games().length > 0}>
              <div class="task-section-title">{t("tasks.sectionGames")}</div>
              <For each={games()}>
                {(game) => (
                  <div class="task-row">
                    <div class="task-row-main">
                      <div class="task-row-title">
                        <span class="dot-running" /> {profileName(game.profileId)}
                      </div>
                      <div class="task-row-subtitle">
                        {t("tasks.game.startedAt", { time: relativeTime(new Date(game.startedAt).toISOString()) })}
                      </div>
                    </div>
                    <div class="task-row-actions">
                      <RmButton size="sm" icon="terminal" onClick={() => uiStore.openLogDrawer(game.profileId)}>
                        {t("tasks.game.log")}
                      </RmButton>
                      <RmButton size="sm" variant="danger" icon="square" onClick={() => void launchStore.kill(game.profileId)}>
                        {t("tasks.game.stop")}
                      </RmButton>
                    </div>
                  </div>
                )}
              </For>
            </Show>

            <Show when={!hasContent()}>
              <div class="task-center-empty">{t("tasks.empty")}</div>
            </Show>

            <Show when={downloadsStore.recent().length > 0}>
              <div class="task-recent">
                <div class="task-section-title">{t("tasks.recent")}</div>
                <For each={downloadsStore.recent().slice(0, 3)}>
                  {(outcome) => (
                    <div class={`task-recent-row ${outcome.ok ? "is-ok" : "is-failed"}`}>
                      <Icon name={outcome.ok ? "check" : "alert"} size={14} />
                      <span class="task-recent-label">{outcome.label}</span>
                      <span class="task-recent-detail">{outcome.detail ?? ""}</span>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}

function profileName(profileId: string): string {
  return profilesStore.items()?.find((p) => p.id === profileId)?.name ?? profileId;
}

function DownloadRow(props: { batch: { batchId: string; label: string; version: string | null; completedFiles: number; totalFiles: number; downloadedBytes: number; totalBytes: number; currentFile: string | null; speed: number; state: string; error?: string } }): JSX.Element {
  const paused = () => downloadsStore.paused().has(props.batch.batchId);
  const indeterminate = () => props.batch.totalBytes === 0;
  const percent = () =>
    props.batch.totalBytes > 0 ? (props.batch.downloadedBytes / props.batch.totalBytes) * 100 : 0;

  async function cancel(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("tasks.cancelConfirm.title"),
      body: t("tasks.cancelConfirm.body"),
      confirmLabel: t("tasks.download.cancel"),
    });
    if (allowed) {
      await downloads.cancel(props.batch.batchId);
      toast({ message: t("tasks.download.cancelled"), tone: "info" });
    }
  }

  return (
    <div class="task-row is-download">
      <div class="task-row-head">
        <div class="task-row-title">{props.batch.label || props.batch.version}</div>
        <div class="task-row-actions">
          <Show when={!indeterminate()}>
            <RmIconButton
              icon={paused() ? "play" : "pause"}
              label={paused() ? t("tasks.download.resume") : t("tasks.download.pause")}
              onClick={() => {
                if (paused()) {
                  downloadsStore.resume(props.batch.batchId);
                  void downloads.resume(props.batch.batchId);
                } else {
                  downloadsStore.pause(props.batch.batchId);
                  void downloads.pause(props.batch.batchId);
                }
              }}
            />
            <RmIconButton icon="x" label={t("tasks.download.cancel")} danger onClick={() => void cancel()} />
          </Show>
        </div>
      </div>
      <RmProgress value={percent()} determinate={!indeterminate()} />
      <div class="task-row-meta">
        <Show when={props.batch.speed > 0}>
          <span class="task-row-speed">{formatBytes(props.batch.speed)}/s</span>
        </Show>
        <span>
          {groupDigits(props.batch.completedFiles)}/{groupDigits(props.batch.totalFiles)} ·{" "}
          {formatBytes(props.batch.downloadedBytes)}/{formatBytes(props.batch.totalBytes)}
        </span>
      </div>
      <Show when={props.batch.currentFile}>
        <div class="task-row-file">{props.batch.currentFile}</div>
      </Show>
      <Show when={props.batch.error}>
        <div class="task-row-file is-error">{props.batch.error}</div>
      </Show>
    </div>
  );
}

/** Destructive-tool confirmation card (ui-design 07 §3.3). */
function ConfirmationCard(props: { confirmation: McpConfirmationView }): JSX.Element {
  const [countdown, setCountdown] = createSignal(120);
  createMemo(() => {
    const created = Date.parse(props.confirmation.createdAt);
    const tick = setInterval(() => {
      const elapsed = Math.floor((Date.now() - created) / 1000);
      setCountdown(Math.max(0, 120 - elapsed));
    }, 1000);
    setTimeout(() => clearInterval(tick), 125_000);
    return created;
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

/** Per-tool humanized impact line; falls back to the raw JSON args. */
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
      return t("tasks.confirm.impact.launchKill", { id: String(payload.profile_id ?? "") });
    case "settings.set":
      return t("tasks.confirm.impact.settingsSet");
    default:
      return JSON.stringify(payload);
  }
}
