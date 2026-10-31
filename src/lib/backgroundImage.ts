/**
 * Local still-image wallpaper preference and preparation state.
 *
 * The backend validates the selected file and grants asset-protocol access to
 * exactly that path; decoding and scaling stay in the system WebView. This
 * module persists the visual preference and deduplicates the prepare IPC used
 * during bootstrap and by the wallpaper component.
 */
import { createRoot, createSignal } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { backgroundMedia } from "../ipc/api";

export interface BackgroundImagePrefs {
  path: string;
  enabled: boolean;
  /** Black scrim opacity over the image, 0–80 %. */
  dim: number;
}

export interface ResolvedBackgroundImage {
  path: string;
  url: string;
}

const STORAGE_KEY = "rmccc.backgroundImage.v1";
const DEFAULT_PREFS: BackgroundImagePrefs = { path: "", enabled: false, dim: 20 };

function clampDim(value: number): number {
  return Math.round(Math.min(80, Math.max(0, value)));
}

function readPrefs(): BackgroundImagePrefs {
  try {
    const raw = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? "{}"
    ) as Partial<BackgroundImagePrefs>;
    return {
      path: typeof raw.path === "string" ? raw.path : "",
      enabled: raw.enabled === true,
      dim: Number.isFinite(raw.dim) ? clampDim(Number(raw.dim)) : DEFAULT_PREFS.dim,
    };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

const [prefs, setPrefsSignal] = createRoot(() => createSignal<BackgroundImagePrefs>(readPrefs()));
const [issue, setIssueSignal] = createRoot(() => createSignal(""));
const [preparing, setPreparingSignal] = createRoot(() => createSignal(false));
const [asset, setAssetSignal] = createRoot(() =>
  createSignal<ResolvedBackgroundImage | null>(null)
);

let generation = 0;
const completed = new Map<string, ResolvedBackgroundImage>();
const pending = new Map<string, Promise<ResolvedBackgroundImage | null>>();

function persist(next: BackgroundImagePrefs): void {
  setPrefsSignal(next);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // The live wallpaper still works when storage is unavailable.
  }
}

function invalidatePreparation(): void {
  generation++;
  setAssetSignal(null);
  setPreparingSignal(false);
}

export function backgroundImage(): BackgroundImagePrefs {
  return prefs();
}

export function backgroundImageIssue(): string {
  return issue();
}

export function setBackgroundImageIssue(message: string): void {
  setIssueSignal(message);
}

export function backgroundImagePreparing(): boolean {
  return preparing();
}

export function backgroundImageAsset(): ResolvedBackgroundImage | null {
  return asset();
}

export function setBackgroundImageEnabled(enabled: boolean): void {
  if (!enabled) invalidatePreparation();
  persist({ ...prefs(), enabled });
}

export function setBackgroundImageDim(dim: number): void {
  persist({ ...prefs(), dim: clampDim(dim) });
}

export function clearBackgroundImage(): void {
  invalidatePreparation();
  setIssueSignal("");
  persist({ ...prefs(), path: "", enabled: false });
}

/** Open the native picker. Cancelling leaves the current choice untouched. */
export async function pickBackgroundImage(): Promise<{
  ok: boolean;
  message?: string;
  selected?: boolean;
}> {
  const res = await backgroundMedia.pickImage();
  if (!res.ok) return { ok: false, message: res.error.message };
  if (!res.data) return { ok: true, selected: false };
  invalidatePreparation();
  setIssueSignal("");
  persist({ ...prefs(), path: res.data, enabled: true });
  return { ok: true, selected: true };
}

/**
 * Prepare and authorize the selected image exactly once per path. A rejected
 * preparation is not cached, so reselecting can retry it. Stale completions
 * cannot replace a newer wallpaper or clear its error state.
 */
export function primeBackgroundImage(): Promise<ResolvedBackgroundImage | null> {
  const current = prefs();
  if (!current.enabled || !current.path) return Promise.resolve(null);

  const cached = completed.get(current.path);
  if (cached) {
    setAssetSignal(cached);
    return Promise.resolve(cached);
  }
  const existing = pending.get(current.path);
  if (existing) return existing;

  const path = current.path;
  const ticket = ++generation;
  setPreparingSignal(true);
  setIssueSignal("");
  const promise = (async (): Promise<ResolvedBackgroundImage | null> => {
    try {
      const res = await backgroundMedia.prepareImage(path);
      if (!res.ok) {
        if (ticket === generation) setIssueSignal(res.error.message);
        return null;
      }
      const resolved = { path: res.data, url: convertFileSrc(res.data) };
      completed.set(path, resolved);
      if (ticket === generation) {
        setAssetSignal(resolved);
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
