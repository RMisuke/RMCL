/**
 * Launch store: running processes + per-profile log ring buffers (ui-design 10 §4).
 * 1000 lines per profile; buffers survive exit until the drawer closes.
 */
import { createRoot, createSignal } from "solid-js";
import { launch } from "../ipc/api";
import { EVENTS } from "../ipc/events";
import type { LaunchProgressPayload } from "../ipc/types";
import { listen } from "@tauri-apps/api/event";
import { instancesStore } from "./instances";
import { uiStore } from "./ui";
import { t } from "../i18n";

const MAX_LINES = 1000;

/**
 * User-facing launch errors: the backend messages are English/technical, so
 * the two Java preconditions get localized here (the numbers come from the
 * structured message the backend formats deterministically).
 */
function launchErrorMessage(error: { code: string; message: string }): string {
  if (error.code === "java_not_found") return t("errors.javaNotFound");
  if (error.code === "java_version_mismatch") {
    const match = /found Java (\d+).*?Java (\d+)/.exec(error.message);
    if (match) {
      return t("errors.javaVersionMismatch", { found: match[1], required: match[2] });
    }
  }
  return error.message;
}

export interface RunningGame {
  profileId: string;
  pid: number;
  startedAt: number;
}

export interface LaunchExit {
  profileId: string;
  exitCode: number;
  exitType: string;
}

/**
 * Launch flow shown in the bottom-right bubble (HMCL launching-steps style):
 * check instance resources → repair if incomplete → start → success.
 */
export type LaunchFlowStage =
  | "check"
  | "repair"
  | "launching"
  | "waiting"
  | "started"
  | "failed";

export interface LaunchFlow {
  profileId: string;
  profileName: string;
  stage: LaunchFlowStage;
  percent: number;
  message?: string;
  error?: string;
}

export type LaunchProgress = LaunchProgressPayload;

export const launchStore = createRoot(() => {
  const [running, setRunning] = createSignal<Map<string, RunningGame>>(new Map());
  const [buffers, setBuffers] = createSignal<Map<string, string[]>>(new Map());
  const [lastExit, setLastExit] = createSignal<LaunchExit | null>(null);
  const [starting, setStarting] = createSignal<Set<string>>(new Set());
  /** Cleared when its drawer closes (ui-design 10 §4). */
  const keepBuffers = new Set<string>();
  /** Profiles the user stopped from the UI — distinguishes kill from crash (P1-7). */
  const userKilled = new Set<string>();
  const [flow, setFlow] = createSignal<LaunchFlow | null>(null);
  const [progress, setProgress] = createSignal<Map<string, LaunchProgress>>(new Map());
  let flowClearTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleFlowClear(delayMs: number): void {
    if (flowClearTimer) clearTimeout(flowClearTimer);
    flowClearTimer = setTimeout(() => {
      setFlow(null);
      flowClearTimer = null;
    }, delayMs);
  }

  function isRunning(profileId: string): boolean {
    return running().has(profileId);
  }

  function isStarting(profileId: string): boolean {
    return starting().has(profileId);
  }

  function appendLines(profileId: string, lines: string[]): void {
    setBuffers((map) => {
      const next = new Map(map);
      const existing = next.get(profileId) ?? [];
      const merged = [...existing, ...lines].slice(-MAX_LINES);
      next.set(profileId, merged);
      return next;
    });
  }

  async function start(profileId: string, profileName?: string): Promise<{ ok: boolean; message?: string }> {
    setStarting((set) => new Set(set).add(profileId));

    // Launching always makes this instance the active one: the Home launch
    // picker and its running card must follow a launch started from the
    // instance list/detail page, not keep showing a stale selection.
    const known = (instancesStore.items() ?? []).some((item) => item.summary.key === profileId);
    try {
      await instancesStore.selectInstance(profileId);
      if (!known) {
        await Promise.all([instancesStore.refresh(true), instancesStore.refreshDirectories(true)]);
      }
    } catch {
      // Selection is cosmetic; never block a launch on it.
    }

    // 启动气泡只展示后端上报的真实阶段；不再先跑一遍 check/repair
    // （launch_start 内部已经执行 ensure_launch_ready，重复检查会拖慢启动并
    // 让两套进度互相覆盖）。
    setProgress((map) => {
      const next = new Map(map);
      next.delete(profileId);
      return next;
    });
    setFlow({
      profileId,
      profileName: profileName ?? profileId,
      stage: "check",
      percent: 0,
      message: t("launchFlow.detail.checking"),
    });
    const res = await launch.start(profileId);
    setStarting((set) => {
      const next = new Set(set);
      next.delete(profileId);
      return next;
    });
    if (!res.ok) {
      const message = launchErrorMessage(res.error);
      setFlow((f) =>
        f && f.profileId === profileId
          ? { ...f, stage: "failed", error: message, message }
          : f
      );
      // P2-19: the failed bubble stays until dismissed — a 5s auto-clear
      // dropped the error whenever the user glanced away.
      return { ok: false, message };
    }
    // Attribute the launch to the active account (account card last-played).
    void import("./auth").then(({ authStore }) => authStore.recordAccountActivity());
    return { ok: true };
  }

  function stageForFlow(stage: string): LaunchFlowStage {
    switch (stage) {
      case "repairing":
        return "repair";
      case "checking-java":
      case "spawning":
        return "launching";
      case "waiting-window":
        return "waiting";
      case "window-open":
        return "started";
      default:
        return "check";
    }
  }

  /**
   * Process spawn is **not** 100%: the game window has not been observed yet.
   * Keep the bubble in "waiting for window" until the backend's stdout marker
   * arrives.
   */
  function markProcessStarted(profileId: string): void {
    setFlow((f) => {
      if (!f || f.profileId !== profileId || f.stage === "failed" || f.stage === "started") {
        return f;
      }
      return {
        ...f,
        stage: "waiting",
        percent: Math.max(f.percent, 88),
        message: t("launchFlow.detail.waitingWindow"),
      };
    });
  }

  /** Window-open event: the only transition to 100%. */
  function markWindowOpened(profileId: string, message?: string): void {
    setFlow((f) => {
      if (!f || f.profileId !== profileId) return f;
      if (f.stage === "started") return f;
      scheduleFlowClear(2500);
      return {
        ...f,
        stage: "started",
        percent: 100,
        message: message ?? t("launchFlow.detail.windowOpen"),
      };
    });
  }

  function applyProgress(event: LaunchProgressPayload): void {
    const percent = Math.max(0, Math.min(100, event.percent));
    setProgress((map) => {
      const previous = map.get(event.profileId);
      // Progress must never move backwards (the 100% marker can arrive before
      // the post-spawn 88% event).
      const merged: LaunchProgress = {
        ...event,
        percent: Math.max(previous?.percent ?? 0, percent),
        windowOpened: previous?.windowOpened || event.windowOpened || percent >= 100,
      };
      if (merged.windowOpened) merged.percent = 100;
      const next = new Map(map);
      next.set(event.profileId, merged);
      return next;
    });

    if (event.windowOpened || percent >= 100) {
      markWindowOpened(event.profileId, event.message);
      return;
    }

    setFlow((f) => {
      if (!f || f.profileId !== event.profileId) return f;
      if (f.stage === "failed" || f.stage === "started") return f;
      return {
        ...f,
        stage: stageForFlow(event.stage),
        percent: Math.max(f.percent, percent),
        message: event.message,
      };
    });
  }

  function progressFor(profileId: string): LaunchProgress | null {
    return progress().get(profileId) ?? null;
  }

  /**
   * Confirm first (P1-11: every UI stop entry point funnels through here),
   * then kill; the exit is marked user-initiated so it is not treated as a
   * crash (P1-7).
   */
  async function stop(profileId: string): Promise<void> {
    const name = instancesStore.items()?.find((p) => p.summary.key === profileId)?.summary.display_name ?? profileId;
    const allowed = await uiStore.confirm({
      title: t("launch.stopConfirm.title"),
      body: t("launch.stopConfirm.body", { name }),
      confirmLabel: t("common.stop"),
      danger: true,
    });
    if (!allowed) return;
    await kill(profileId);
  }

  /** Consume the user-kill marker once; true when this exit was user-initiated. */
  function consumeUserKill(profileId: string): boolean {
    return userKilled.delete(profileId);
  }

  async function kill(profileId: string): Promise<void> {
    userKilled.add(profileId);
    // 乐观更新：UI 立即停止显示运行中；退出事件晚到也无害（幂等）。
    setProgress((map) => {
      const next = new Map(map);
      next.delete(profileId);
      return next;
    });
    setRunning((map) => {
      const next = new Map(map);
      next.delete(profileId);
      return next;
    });
    await launch.kill(profileId);
    // 对账兜底：以后端 running 清单为准（防乐观更新与真实状态漂移）。
    const res = await launch.status();
    if (res.ok) {
      setRunning((map) => {
        const next = new Map<string, RunningGame>();
        for (const id of res.data) {
          next.set(id, map.get(id) ?? { profileId: id, pid: 0, startedAt: Date.now() });
        }
        return next;
      });
    }
  }

  /** Restore running state on app start (ui-design 10 §8). */
  async function restoreStatus(): Promise<void> {
    const res = await launch.status();
    if (!res.ok) return;
    setRunning((current) => {
      const next = new Map<string, RunningGame>();
      for (const profileId of res.data) {
        next.set(profileId, current.get(profileId) ?? { profileId, pid: 0, startedAt: Date.now() });
      }
      return next;
    });
  }

  function attachEvents(): void {
    void listen<{ profile_id: string; pid: number }>(EVENTS.LAUNCH_STARTED, (event) => {
      const { profile_id, pid } = event.payload;
      userKilled.delete(profile_id); // a fresh run clears any stale kill marker
      markProcessStarted(profile_id);
      setRunning((map) => {
        const next = new Map(map);
        next.set(profile_id, { profileId: profile_id, pid, startedAt: Date.now() });
        return next;
      });
    });
    void listen<LaunchProgressPayload>(EVENTS.LAUNCH_PROGRESS, (event) => {
      applyProgress(event.payload);
    });
    // Backend coalesces stdout lines into batches (~10 events/s at high
    // rates) — append the whole batch in one signal update.
    void listen<{ profile_id: string; lines: string[] }>(EVENTS.LAUNCH_OUTPUT, (event) => {
      appendLines(event.payload.profile_id, event.payload.lines);
    });
    void listen<{ profile_id: string; exit_code: number; exit_type: string }>(
      EVENTS.LAUNCH_EXITED,
      (event) => {
        const { profile_id, exit_code, exit_type } = event.payload;
        setRunning((map) => {
          const next = new Map(map);
          next.delete(profile_id);
          return next;
        });
        setProgress((map) => {
          const next = new Map(map);
          next.delete(profile_id);
          return next;
        });
        setFlow((f) =>
          f && f.profileId === profile_id && f.stage !== "started" && f.stage !== "failed"
            ? { ...f, stage: "failed", error: t("launchFlow.detail.exitedEarly"), message: t("launchFlow.detail.exitedEarly") }
            : f
        );
        setLastExit({ profileId: profile_id, exitCode: exit_code, exitType: exit_type });
      }
    );
  }

  /** Drop a buffer (drawer closed). */
  function dropBuffer(profileId: string): void {
    keepBuffers.delete(profileId);
    setBuffers((map) => {
      const next = new Map(map);
      next.delete(profileId);
      return next;
    });
  }

  /** Keep the buffer across drawer close (crash inspection). */
  function keepBuffer(profileId: string): void {
    keepBuffers.add(profileId);
  }

  return {
    running,
    isRunning,
    isStarting,
    starting,
    buffers,
    lastExit,
    clearLastExit: () => setLastExit(null),
    flow,
    progress,
    progressFor,
    dismissFlow: () => {
      if (flowClearTimer) clearTimeout(flowClearTimer);
      flowClearTimer = null;
      setFlow(null);
    },
    start,
    stop,
    kill,
    consumeUserKill,
    restoreStatus,
    attachEvents,
    dropBuffer,
    keepBuffer,
  };
});
