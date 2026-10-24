/**
 * Modloader install store: in-flight loader installs keyed by profile+loader
 * (ui-design 10 §1). Driven by `modloader://progress` stage events.
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
  updatedAt: number;
}

export const modloadersStore = createRoot(() => {
  const [installs, setInstalls] = createSignal<Map<string, LoaderInstall>>(new Map());

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
      setInstalls((map) => {
        const next = new Map(map);
        if (p.stage === "completed" || p.stage === "failed") {
          next.delete(key);
        } else {
          next.set(key, {
            profileId: p.profile_id,
            loaderType: p.loader_type,
            gameVersion: p.game_version,
            stage: p.stage,
            percent: p.percent,
            message: p.message,
            updatedAt: Date.now(),
          });
        }
        return next;
      });
    });
  }

  function forProfile(profileId: string): LoaderInstall[] {
    return [...installs().values()].filter((i) => i.profileId === profileId);
  }

  return {
    installs,
    forProfile,
    attachEvents,
  };
});
