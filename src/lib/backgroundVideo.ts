/**
 * Local MP4 background preference (Appearance).
 *
 * The path lives in localStorage like the glass opacity setting: it is a
 * per-machine visual preference, not launcher configuration. The backend only
 * validates the file and grants the WebView read access for this session.
 */
import { createRoot, createSignal } from "solid-js";
import { backgroundMedia } from "../ipc/api";

export interface BackgroundVideoPrefs {
  path: string;
  enabled: boolean;
  /** Black scrim opacity over the video, 0–80 %. */
  dim: number;
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

function persist(next: BackgroundVideoPrefs): void {
  setPrefsSignal(next);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // The live preview still works when storage is unavailable.
  }
}

export function backgroundVideo(): BackgroundVideoPrefs {
  return prefs();
}

/** Last playback/access problem, surfaced in Settings; cleared on success. */
export function backgroundVideoIssue(): string {
  return issue();
}

export function setBackgroundVideoIssue(message: string): void {
  setIssueSignal(message);
}

export function setBackgroundVideoEnabled(enabled: boolean): void {
  persist({ ...prefs(), enabled });
}

export function setBackgroundVideoDim(dim: number): void {
  persist({ ...prefs(), dim: clampDim(dim) });
}

export function clearBackgroundVideo(): void {
  setIssueSignal("");
  persist({ ...prefs(), path: "", enabled: false });
}

/**
 * Open the native picker and remember the result. Cancelling leaves the
 * current choice untouched (the launcher keeps whatever it had).
 */
export async function pickBackgroundVideo(): Promise<{ ok: boolean; message?: string }> {
  const res = await backgroundMedia.pickVideo();
  if (!res.ok) return { ok: false, message: res.error.message };
  if (!res.data) return { ok: true };
  setIssueSignal("");
  persist({ ...prefs(), path: res.data, enabled: true });
  return { ok: true };
}
