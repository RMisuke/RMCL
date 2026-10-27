/**
 * Launch store: running processes + per-profile log ring buffers (ui-design 10 §4).
 * 1000 lines per profile; buffers survive exit until the drawer closes.
 */
import { createRoot, createSignal } from "solid-js";
import { launch, instances } from "../ipc/api";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";
import { instancesStore } from "./instances";
import { uiStore } from "./ui";
import { t } from "../i18n";

const MAX_LINES = 1000;

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
export interface LaunchFlow {
  profileId: string;
  profileName: string;
  stage: "check" | "repair" | "launching" | "started" | "failed";
  missing?: number;
  repaired?: number;
  error?: string;
}

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
  let flowClearTimer: ReturnType<typeof setTimeout> | null = null;
  let flowStartTimer: ReturnType<typeof setTimeout> | null = null;

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

    // 启动气泡：检查资源 → 自动补全 → 启动（HMCL launching steps 等价）。
    setFlow({ profileId, profileName: profileName ?? profileId, stage: "check" });
    let missing = 0;
    try {
      const check = await instances.checkCompletion(profileId);
      if (check.ok) missing = check.data.length;
    } catch {
      /* 检查失败不阻塞启动——启动流程自带 pre-launch 修复 */
    }
    if (missing > 0) {
      setFlow((f) => (f && f.profileId === profileId ? { ...f, stage: "repair", missing } : f));
      try {
        const repaired = await instances.repair(profileId);
        if (repaired.ok) {
          setFlow((f) => (f && f.profileId === profileId ? { ...f, repaired: repaired.data } : f));
        }
      } catch {
        /* 同上 */
      }
    }
    setFlow((f) => (f && f.profileId === profileId ? { ...f, stage: "launching" } : f));

    const res = await launch.start(profileId);
    setStarting((set) => {
      const next = new Set(set);
      next.delete(profileId);
      return next;
    });
    if (!res.ok) {
      setFlow((f) => (f && f.profileId === profileId ? { ...f, stage: "failed", error: res.error.message } : f));
      // P2-19: the failed bubble stays until dismissed — a 5s auto-clear
      // dropped the error whenever the user glanced away.
      return { ok: false, message: res.error.message };
    }
    // LAUNCH_STARTED 事件通常先到；45s 兜底（进程拉起但事件丢失时仍收尾）。
    // 句柄存下来：dismissFlow 时一并取消（P3-10）。
    flowStartTimer = setTimeout(() => {
      flowStartTimer = null;
      setFlow((f) =>
        f && f.profileId === profileId && f.stage === "launching" ? { ...f, stage: "started" } : f
      );
      scheduleFlowClear(2500);
    }, 45_000);
    // Attribute the launch to the active account (account card last-played).
    void import("./auth").then(({ authStore }) => authStore.recordAccountActivity());
    return { ok: true };
  }

  /** LAUNCH_STARTED 事件到达：气泡进入"启动成功"并在 2.5s 后关闭。 */
  function markStarted(profileId: string): void {
    setFlow((f) => {
      if (!f || f.profileId !== profileId || f.stage === "failed" || f.stage === "started") return f;
      scheduleFlowClear(2500);
      return { ...f, stage: "started" };
    });
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
      markStarted(profile_id);
      setRunning((map) => {
        const next = new Map(map);
        next.set(profile_id, { profileId: profile_id, pid, startedAt: Date.now() });
        return next;
      });
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
    dismissFlow: () => {
      if (flowClearTimer) clearTimeout(flowClearTimer);
      flowClearTimer = null;
      if (flowStartTimer) clearTimeout(flowStartTimer);
      flowStartTimer = null;
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
