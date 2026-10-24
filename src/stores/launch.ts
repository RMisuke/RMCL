/**
 * Launch store: running processes + per-profile log ring buffers (ui-design 10 §4).
 * 1000 lines per profile; buffers survive exit until the drawer closes.
 */
import { createRoot, createSignal } from "solid-js";
import { launch, instances } from "../ipc/api";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

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
  const [flow, setFlow] = createSignal<LaunchFlow | null>(null);
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
      scheduleFlowClear(5000);
      return { ok: false, message: res.error.message };
    }
    // LAUNCH_STARTED 事件通常先到；45s 兜底（进程拉起但事件丢失时仍收尾）。
    setTimeout(() => {
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

  async function kill(profileId: string): Promise<void> {
    await launch.kill(profileId);
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
      markStarted(profile_id);
      setRunning((map) => {
        const next = new Map(map);
        next.set(profile_id, { profileId: profile_id, pid, startedAt: Date.now() });
        return next;
      });
    });
    void listen<{ profile_id: string; text: string }>(EVENTS.LAUNCH_OUTPUT, (event) => {
      appendLines(event.payload.profile_id, [event.payload.text]);
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
      setFlow(null);
    },
    start,
    kill,
    restoreStatus,
    attachEvents,
    dropBuffer,
    keepBuffer,
  };
});
