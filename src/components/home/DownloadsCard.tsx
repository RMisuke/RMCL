/**
 * Home download/task card. It is a compact view over the unified task centre:
 * the same TaskSnapshot, the same pause/resume/cancel commands, and a direct
 * route into the full task-centre panel.
 */
import { For, Show, createMemo } from "solid-js";
import type { JSX } from "solid-js";
import { Icon } from "../icons";
import { RmIconButton, RmProgress } from "../rm/controls";
import { formatBytes } from "../../lib/format";
import { t } from "../../i18n";
import { confirmDanger, toast, uiStore } from "../../stores/ui";
import { tasksStore } from "../../stores/tasks";
import type { TaskSnapshot } from "../../ipc/types";

function taskPercent(task: TaskSnapshot): number {
  return Math.max(0, Math.min(100, task.overallPercent));
}

function taskDetail(task: TaskSnapshot): string {
  if (task.download) {
    return `${task.download.completedFiles}/${task.download.totalFiles} · ${formatBytes(task.download.downloadedBytes)}/${formatBytes(task.download.totalBytes)}`;
  }
  return task.currentStepLabel ?? task.message;
}

export function DownloadsCard(): JSX.Element {
  const active = createMemo(() => tasksStore.activeList());
  const recent = createMemo(() => tasksStore.recent().slice(0, 2));
  const aggregate = createMemo(() => tasksStore.aggregate());
  const openCenter = () => uiStore.setTaskCenterOpen(true);

  async function cancel(task: TaskSnapshot): Promise<void> {
    const allowed = await confirmDanger({
      title: t("tasks.cancelConfirm.title"),
      body: t("tasks.cancelConfirm.body"),
      confirmLabel: t("tasks.download.cancel"),
    });
    if (!allowed) return;
    await tasksStore.cancel(task.id);
    toast({ message: t("tasks.download.cancelled"), tone: "info" });
  }

  async function toggle(task: TaskSnapshot): Promise<void> {
    if (task.state === "paused") await tasksStore.resume(task.id);
    else await tasksStore.pause(task.id);
  }

  return (
    <div class="soft-downloads is-task-linked">
      <div class="soft-download-toolbar">
        <span>
          {t("home.downloads.activeSummary", { n: String(active().length) })}
          <Show when={aggregate().speed > 0}>
            {` · ${formatBytes(aggregate().speed)}/s`}
          </Show>
        </span>
        <button type="button" class="soft-task-center-link" onClick={openCenter}>
          {t("home.downloads.openCenter")}
          <Icon name="chevron-right" size={12} />
        </button>
      </div>

      <Show when={active().length > 0} fallback={
        <Show when={recent().length > 0} fallback={
          <button type="button" class="soft-empty is-link" onClick={openCenter}>
            <Icon name="check" size={17} />
            {t("home.downloads.empty")}
          </button>
        }>
          <div class="soft-task-recent">
            <span class="soft-task-recent-label">{t("home.downloads.recent")}</span>
            <For each={recent()}>
              {(task) => (
                <button type="button" class="soft-task-recent-row" onClick={openCenter}>
                  <span>{task.title}</span>
                  <span class={`soft-task-state is-${task.state}`}>{t(`tasks.state.${task.state}`)}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      }>
        <For each={active().slice(0, 3)}>
          {(task) => (
            <div
              class="soft-task-row"
              role="button"
              tabIndex={0}
              onClick={openCenter}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") openCenter();
              }}
            >
              <div class="soft-task-copy">
                <strong>{task.title}</strong>
                <small>{task.currentStepLabel ?? task.message}</small>
                <small class="soft-task-counters">{taskDetail(task)}</small>
              </div>
              <RmProgress value={taskPercent(task)} determinate={taskPercent(task) > 0} />
              <span class="soft-task-percent">{Math.round(taskPercent(task))}%</span>
              <div class="soft-task-actions" onClick={(event) => event.stopPropagation()}>
                <Show when={task.capabilities.canPause && task.state === "running"}>
                  <RmIconButton
                    icon="pause"
                    label={t("tasks.download.pause")}
                    onClick={() => void toggle(task)}
                  />
                </Show>
                <Show when={task.capabilities.canResume && task.state === "paused"}>
                  <RmIconButton
                    icon="play"
                    label={t("tasks.download.resume")}
                    onClick={() => void toggle(task)}
                  />
                </Show>
                <Show when={task.capabilities.canCancel}>
                  <RmIconButton
                    icon="x"
                    label={t("tasks.download.cancel")}
                    danger
                    onClick={() => void cancel(task)}
                  />
                </Show>
              </div>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
