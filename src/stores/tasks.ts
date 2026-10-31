/**
 * Unified task-center store.
 *
 * One source of truth: `TaskSnapshot` from `task://updated`, with
 * `task_list` for the initial board and `task://sync` after a lag. There are
 * no synthetic tasks and no batch-id re-keying — a task exists the moment the
 * backend schedules it and keeps the same id until it reaches history.
 */
import { createRoot, createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { listen } from "@tauri-apps/api/event";
import { tasks } from "../ipc/api";
import { EVENTS } from "../ipc/events";
import type { TaskBoard, TaskSnapshot, TaskState } from "../ipc/types";
import { instancesStore } from "./instances";
import { toast } from "./ui";
import { t } from "../i18n";

export function isTerminal(state: TaskState): boolean {
  return state === "succeeded" || state === "failed" || state === "cancelled";
}

export const tasksStore = createRoot(() => {
  const [active, setActive] = createStore<Record<string, TaskSnapshot>>({});
  const [recent, setRecent] = createSignal<TaskSnapshot[]>([]);
  const [hydrated, setHydrated] = createSignal(false);
  // Terminal tasks are broadcast once, but a duplicate snapshot after a resync
  // must not show a second completion toast.
  const seen = new Set<string>();

  function activeList(): TaskSnapshot[] {
    return Object.values(active).sort((a, b) => Number(a.createdAt) - Number(b.createdAt));
  }

  function byKind(kind: TaskSnapshot["kind"]): TaskSnapshot[] {
    return activeList().filter((task) => task.kind === kind);
  }

  function forTarget(target: string): TaskSnapshot[] {
    return activeList().filter((task) => task.target === target);
  }

  function aggregate(): { downloaded: number; total: number; speed: number; percent: number } {
    let downloaded = 0;
    let total = 0;
    let speed = 0;
    let percentSum = 0;
    for (const task of activeList()) {
      if (task.download) {
        downloaded += task.download.downloadedBytes;
        total += task.download.totalBytes;
        speed += task.download.speedBps;
      }
      percentSum += task.overallPercent;
    }
    const list = activeList();
    return {
      downloaded,
      total,
      speed,
      percent: list.length > 0 ? percentSum / list.length : 0,
    };
  }

  function noteCompletion(task: TaskSnapshot): void {
    if (seen.has(task.id)) return;
    seen.add(task.id);
    if (task.state === "succeeded") {
      if (
        task.kind === "instance-install" ||
        task.kind === "modpack-install" ||
        task.kind === "modloader-install"
      ) {
        void instancesStore.refresh(true);
        void instancesStore.refreshDirectories(true);
        const key = task.result?.key;
        const manual = task.result?.manualDirectory;
        if (manual) void instancesStore.selectDirectory(manual);
        else if (key) void instancesStore.selectInstance(key);
      }
      if (task.kind === "instance-install" || task.kind === "modpack-install") {
        toast({ message: task.message || t("tasks.completed"), tone: "success" });
        if (task.warnings.length > 0) {
          toast({
            message: t("tasks.completedWarnings", {
              n: String(task.warnings.length),
              detail: task.warnings[0],
            }),
            tone: "info",
          });
        }
      }
    } else if (task.state === "failed") {
      // Synchronous commands (mod install / repair / dependency apply) already
      // report their own errors; only detached installs need the global toast.
      if (task.kind === "instance-install" || task.kind === "modpack-install") {
        toast({ message: task.error?.message ?? task.message, tone: "error" });
      }
    }
  }

  function apply(task: TaskSnapshot, live: boolean): void {
    if (isTerminal(task.state)) {
      setActive(
        produce((all) => {
          delete all[task.id];
        })
      );
      setRecent((list) => [task, ...list.filter((item) => item.id !== task.id)].slice(0, 32));
      if (live) noteCompletion(task);
      else seen.add(task.id);
      return;
    }
    setActive(task.id, task);
  }

  function applyBoard(board: TaskBoard): void {
    setActive(
      produce((all) => {
        for (const key of Object.keys(all)) delete all[key];
        for (const task of board.active) all[task.id] = task;
      })
    );
    setRecent(board.recent);
    for (const task of board.recent) seen.add(task.id);
    setHydrated(true);
  }

  async function hydrate(): Promise<void> {
    const res = await tasks.list();
    if (res.ok) applyBoard(res.data);
    else setHydrated(true);
  }

  async function cancel(taskId: string): Promise<void> {
    const res = await tasks.cancel(taskId);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  async function pause(taskId: string): Promise<void> {
    const res = await tasks.pause(taskId);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  async function resume(taskId: string): Promise<void> {
    const res = await tasks.resume(taskId);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  async function clearHistory(): Promise<void> {
    const res = await tasks.clearHistory();
    if (res.ok) setRecent([]);
    else toast({ message: res.error.message, tone: "error" });
  }

  async function log(taskId: string): Promise<string[]> {
    const res = await tasks.log(taskId);
    return res.ok ? res.data : [];
  }

  function attachEvents(): void {
    void listen<TaskSnapshot>(EVENTS.TASK_UPDATED, (event) => apply(event.payload, true));
    void listen<TaskBoard>(EVENTS.TASK_SYNC, (event) => applyBoard(event.payload));
    void hydrate();
  }

  return {
    active,
    recent,
    hydrated,
    activeList,
    byKind,
    forTarget,
    aggregate,
    activeCount: () => Object.keys(active).length,
    cancel,
    pause,
    resume,
    clearHistory,
    log,
    attachEvents,
  };
});
