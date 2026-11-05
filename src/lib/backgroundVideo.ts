/**
 * Local native-resolution MP4 wallpaper preference and preparation state.
 *
 * The backend owns probing, hardware-safe conversion, validation and asset
 * access. This module only persists the visual preference and deduplicates the
 * single prepare IPC started during bootstrap and consumed by the component.
 */
import { createRoot, createSignal } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { backgroundMedia } from "../ipc/api";
import type { BackgroundVideoAsset } from "../ipc/types";
import { backgroundImage } from "./backgroundImage";

export interface BackgroundVideoPrefs {
  path: string;
  enabled: boolean;
  /** Black scrim opacity over the video, 0–80 %. */
  dim: number;
}

export interface ResolvedBackgroundVideo extends BackgroundVideoAsset {
  url: string;
}

const STORAGE_KEY = "rmccc.backgroundVideo.v1";
const DEFAULT_PREFS: BackgroundVideoPrefs = { path: "", enabled: false, dim: 20 };

function clampDim(value: number): number {
  return Math.round(Math.min(80, Math.max(0, value)));
}

function readPrefs(): BackgroundVideoPrefs {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<BackgroundVideoPrefs>;
    return {
      path: typeof raw.path === "string" ? raw.path : "",
      enabled: raw.enabled === true,
      dim: Number.isFinite(raw.dim) ? clampDim(Number(raw.dim)) : DEFAULT_PREFS.dim,
    };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

const [prefs, setPrefsSignal] = createRoot(() => createSignal<BackgroundVideoPrefs>(readPrefs()));
const [issue, setIssueSignal] = createRoot(() => createSignal(""));
const [preparing, setPreparingSignal] = createRoot(() => createSignal(false));

let generation = 0;
const completed = new Map<string, ResolvedBackgroundVideo>();
const pending = new Map<string, Promise<ResolvedBackgroundVideo | null>>();

function persist(next: BackgroundVideoPrefs): void {
  setPrefsSignal(next);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // The live preview still works when storage is unavailable.
  }
}

function invalidatePreparation(): void {
  generation++;
  setPreparingSignal(false);
}

export function backgroundVideo(): BackgroundVideoPrefs {
  return prefs();
}

export function backgroundVideoIssue(): string {
  return issue();
}

export function setBackgroundVideoIssue(message: string): void {
  setIssueSignal(message);
}

export function backgroundVideoPreparing(): boolean {
  return preparing();
}

/** Apply a file selected from the centralized Background catalog. */
export function applyBackgroundVideo(path: string, enabled: boolean, dim: number): void {
  invalidatePreparation();
  setIssueSignal("");
  persist({ path, enabled, dim: clampDim(dim) });
}

/**
 * Prepare and authorize the selected video exactly once per path. A rejected
 * preparation is not cached, so toggling or reselecting can retry it. Stale
 * completions cannot replace a newer wallpaper or clear its error state.
 */
export function primeBackgroundVideo(): Promise<ResolvedBackgroundVideo | null> {
  const current = prefs();
  if (!current.enabled || !current.path) return Promise.resolve(null);
  // The still image is the top wallpaper layer; skip video preparation while
  // it is active so a hidden MP4 never decodes in the background.
  if (backgroundImage().enabled) return Promise.resolve(null);
  try {
    if (localStorage.getItem("rmccc.motionEnabled") === "0") return Promise.resolve(null);
  } catch {
    // Storage access is optional; the normal preference remains usable.
  }

  const cached = completed.get(current.path);
  if (cached) {
    return Promise.resolve(cached);
  }
  const existing = pending.get(current.path);
  if (existing) return existing;

  const path = current.path;
  const ticket = ++generation;
  setPreparingSignal(true);
  setIssueSignal("");
  const promise = (async (): Promise<ResolvedBackgroundVideo | null> => {
    try {
      const res = await backgroundMedia.prepare(path);
      if (!res.ok) {
        if (ticket === generation) setIssueSignal(res.error.message);
        return null;
      }
      const resolved = { ...res.data, url: convertFileSrc(res.data.path) };
      completed.set(path, resolved);
      if (ticket === generation) {
        setIssueSignal("");
      }
      return resolved;
    } catch (error) {
      if (ticket === generation) setIssueSignal(String(error));
      return null;
    } finally {
      if (ticket === generation) setPreparingSignal(false);
    }
  })();

  pending.set(path, promise);
  void promise.finally(() => pending.delete(path));
  return promise;
}
