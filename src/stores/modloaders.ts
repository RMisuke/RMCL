/**
 * Modloader install store: in-flight loader installs keyed by profile+loader
 * (ui-design 10 §1). Driven by `modloader://progress` stage events.
 *
 * 安装细节（2026-10-25）：后端逐阶段上报 `download`（下载字节）/ `run` +
 * `detail`（安装器 stdout 原行）/ `detect` / `repair` 等阶段；store 保留
 * 最近 8 条细节行供 Task Center 展开，完成/失败转入 recent 摘要。
 */
import { createRoot, createSignal } from "solid-js";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

export interface LoaderInstall {
  profileId: string;
  loaderType: string;
  gameVersion: string;
  stage: string;
  percent: number;
  message: string;
  /** 最近的细节行（安装器 stdout / 下载字节进度等），最新在末尾。 */
  details: string[];
  updatedAt: number;
}

export interface LoaderOutcome {
  profileId: string;
  loaderType: string;
  gameVersion: string;
  ok: boolean;
  message: string;
  at: number;
}

const DETAILS_CAP = 8;

export const modloadersStore = createRoot(() => {
  const [installs, setInstalls] = createSignal<Map<string, LoaderInstall>>(new Map());
  const [recent, setRecent] = createSignal<LoaderOutcome[]>([]);

  function keyOf(profileId: string, loaderType: string): string {
    return `${profileId}:${loaderType}`;
  }

  function attachEvents(): void {
    void listen<{
      profile_id: string;
      loader_type: string;
      game_version: string;
      stage: string;
      percent: number;
      message: string;
    }>(EVENTS.MODLOADER_INSTALL_PROGRESS, (event) => {
      const p = event.payload;
      const key = keyOf(p.profile_id, p.loader_type);
      if (p.stage === "completed" || p.stage === "failed") {
        setInstalls((map) => {
          const next = new Map(map);
          next.delete(key);
          return next;
        });
        setRecent((list) =>
          [
            {
              profileId: p.profile_id,
              loaderType: p.loader_type,
              gameVersion: p.game_version,
              ok: p.stage === "completed",
              message: p.message,
              at: Date.now(),
            },
            ...list,
          ].slice(0, 5)
        );
        return;
      }
      setInstalls((map) => {
        const next = new Map(map);
        const current = next.get(key);
        next.set(key, {
          profileId: p.profile_id,
          loaderType: p.loader_type,
          gameVersion: p.game_version,
          stage: p.stage,
          percent: p.percent,
          message: p.message,
          // 细节阶段追加成日志；阶段行覆盖当前 message 即可。
          details:
            p.stage === "detail"
              ? [...(current?.details ?? []), p.message].slice(-DETAILS_CAP)
              : (current?.details ?? []),
          updatedAt: Date.now(),
        });
        return next;
      });
    });
  }

  function forProfile(profileId: string): LoaderInstall[] {
    return [...installs().values()].filter((i) => i.profileId === profileId);
  }

  return {
    installs,
    recent,
    forProfile,
    attachEvents,
  };
});
