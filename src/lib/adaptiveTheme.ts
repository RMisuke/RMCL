/**
 * Adaptive appearance v2 (Noctis).
 *
 * Sampling is deliberately broad for still images (full frame + centre crop +
 * two diagonal detail crops). Videos are read as a **single** live frame — no
 * polling and no hidden clone decoder — and the clusters are computed in a
 * Web Worker so a wallpaper change never blocks the main thread.
 *
 * The sampled clusters then feed `buildAdaptivePalette`, which selects
 * representative roles (prevalence × chroma × hue separation) and builds a
 * dark or light palette that follows the active theme mode.
 */
import { createRoot, createSignal } from "solid-js";
import {
  buildAdaptivePalette,
  extractClustersFromFrames,
  paletteFromThemeAccent,
  type AdaptiveMode,
  type AdaptivePalette,
  type AdaptiveSource,
  type FrameSample,
} from "./adaptivePalette";
import {
  applyThemePreference,
  applyThemeTokens,
  currentThemeTokens,
  getThemePreference,
  themeMode,
} from "../theme";
import { reportFrontendMessage } from "../ipc/api";

const ENABLED_KEY = "rmccc.adaptiveTheme.v1";
const PALETTE_KEY = "rmccc.adaptivePalette.v2";
const SAMPLE_EDGE = 112;
const MEDIA_TIMEOUT_MS = 5200;

export type AdaptiveStatus = "idle" | "sampling" | "ready" | "fallback";
export type { AdaptiveMode, AdaptivePalette };

export function readAdaptiveEnabled(): boolean {
  try {
    return localStorage.getItem(ENABLED_KEY) === "1";
  } catch {
    return false;
  }
}

function readStoredPalette(): AdaptivePalette | null {
  try {
    const raw = localStorage.getItem(PALETTE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AdaptivePalette;
    if (
      parsed?.version !== 2 ||
      (parsed.mode !== "dark" && parsed.mode !== "light") ||
      !Array.isArray(parsed.colors) ||
      parsed.colors.length < 3 ||
      typeof parsed.vars !== "object" ||
      parsed.vars === null ||
      typeof parsed.checks !== "object"
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

const [enabled, setEnabledSignal] = createRoot(() => createSignal(readAdaptiveEnabled()));
const [palette, setPaletteSignal] = createRoot(() => createSignal<AdaptivePalette | null>(readStoredPalette()));
const [status, setStatusSignal] = createRoot(() => createSignal<AdaptiveStatus>(
  readAdaptiveEnabled() ? "ready" : "idle"
));

const appliedVars = new Set<string>();
let sampling: Promise<void> | null = null;
let retryTimer: ReturnType<typeof setTimeout> | null = null;

/** Palettes already computed for a concrete media URL (per session). */
const sampledKeys = new Map<string, AdaptivePalette>();
let paletteWorker: Worker | null = null;
let paletteWorkerUnavailable = false;
let paletteRequest = 0;

export function adaptiveEnabled(): boolean {
  return enabled();
}

export function adaptivePalette(): AdaptivePalette | null {
  return palette();
}

export function adaptiveStatus(): AdaptiveStatus {
  return status();
}

export function adaptiveSource(): AdaptiveSource | null {
  return palette()?.source ?? null;
}

function setRootVar(name: string, value: string): void {
  document.documentElement.style.setProperty(name, value);
  appliedVars.add(name);
}

function clearAdaptiveVars(): void {
  const root = document.documentElement;
  for (const name of appliedVars) root.style.removeProperty(name);
  appliedVars.clear();
  delete root.dataset.adaptive;
  delete root.dataset.adaptiveMode;
}

function safeColorVars(mode: AdaptiveMode): Record<string, string> {
  if (mode === "dark") {
    return {
      "--color-red": "#ff6b6b",
      "--color-red-light": "rgba(255,107,107,0.12)",
      "--color-red-border": "rgba(255,107,107,0.4)",
      "--color-green": "#4ade80",
      "--color-yellow": "#f5a524",
    };
  }
  return {
    "--color-red": "#c4362c",
    "--color-red-light": "#fdf1f0",
    "--color-red-border": "#e9a49f",
    "--color-green": "#1f7a4a",
    "--color-yellow": "#a16207",
  };
}

function tokenVars(p: AdaptivePalette): Record<string, string> {
  const v = p.vars;
  const accent = v["--adaptive-accent"];
  const secondary = v["--adaptive-accent-secondary"];
  const tertiary = v["--adaptive-accent-tertiary"];
  const panel = v["--adaptive-panel"];
  const raised = v["--adaptive-raised"];
  const canvas = v["--adaptive-canvas-1"];
  const ink = v["--adaptive-ink"];
  const muted = v["--adaptive-muted"];
  const line = v["--adaptive-line"];
  const accentContrast = v["--adaptive-accent-contrast"];
  return {
    "--rmccc-background": canvas,
    "--rmccc-surface": panel,
    "--rmccc-surface-elevated": raised,
    "--rmccc-text": ink,
    "--rmccc-text-muted": muted,
    "--rmccc-accent": accent,
    "--rmccc-accent-contrast": accentContrast,
    "--rmccc-border": line,
    "--rmccc-accent-secondary": secondary,
    "--rmccc-accent-tertiary": tertiary,
    "--rmccc-accent-gradient": `linear-gradient(135deg, ${accent}, ${secondary})`,
    "--scene-primary": accent,
    "--scene-secondary": secondary,
    "--scene-tertiary": tertiary,
    "--scene-wash": `color-mix(in srgb, ${accent} 13%, transparent)`,
    "--scene-wash-2": `color-mix(in srgb, ${secondary} 10%, transparent)`,
    "--scene-line": `color-mix(in srgb, ${accent} 24%, transparent)`,
  };
}

function applyPalette(p: AdaptivePalette): void {
  clearAdaptiveVars();
  for (const [name, value] of Object.entries(p.vars)) setRootVar(name, value);
  for (const [name, value] of Object.entries(tokenVars(p))) setRootVar(name, value);
  for (const [name, value] of Object.entries(safeColorVars(p.mode))) setRootVar(name, value);
  const root = document.documentElement;
  root.dataset.adaptive = "on";
  root.dataset.adaptiveMode = p.mode;
  root.dataset.themeMode = p.mode;
  root.dataset.visualTheme = "adaptive";
}

function storePalette(p: AdaptivePalette): void {
  try {
    localStorage.setItem(PALETTE_KEY, JSON.stringify(p));
  } catch {
    /* best-effort */
  }
}

function storeEnabled(value: boolean): void {
  try {
    localStorage.setItem(ENABLED_KEY, value ? "1" : "0");
  } catch {
    /* best-effort */
  }
}

function report(message: string, level: "info" | "warn" = "info"): void {
  void reportFrontendMessage(`adaptive theme: ${message}`, level);
}

/** The concrete media URL currently on screen, used as the palette cache key. */
function currentMediaKey(): string | null {
  if (typeof document === "undefined") return null;
  const media =
    document.querySelector<HTMLImageElement>(".app-image-layer img") ??
    document.querySelector<HTMLVideoElement>(".app-video-layer video") ??
    document.querySelector<HTMLImageElement>(".theme-media-image, .theme-media-poster") ??
    document.querySelector<HTMLVideoElement>(".theme-media-video");
  const src = media?.currentSrc || media?.src || "";
  return src || null;
}

function getPaletteWorker(): Worker | null {
  if (typeof Worker === "undefined" || paletteWorkerUnavailable) return null;
  if (paletteWorker) return paletteWorker;
  try {
    paletteWorker = new Worker(new URL("./adaptiveWorker.ts", import.meta.url), {
      type: "module",
    });
    return paletteWorker;
  } catch {
    paletteWorkerUnavailable = true;
    paletteWorker = null;
    return null;
  }
}

interface WorkerPaletteResult {
  palette: AdaptivePalette;
  ready: boolean;
}

/**
 * Compute the palette off the main thread. Resolves `null` when the worker is
 * unavailable or fails; the caller then falls back without re-reading pixels
 * (their buffers are transferred once posted).
 */
function computePaletteInWorker(
  frames: FrameSample[],
  source: AdaptiveSource,
  options: { mode: AdaptiveMode; frameCount: number; sampleCount: number }
): Promise<WorkerPaletteResult | null> {
  const worker = getPaletteWorker();
  if (!worker) return Promise.resolve(null);
  const id = ++paletteRequest;
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: WorkerPaletteResult | null) => {
      if (settled) return;
      settled = true;
      worker.removeEventListener("message", onMessage);
      worker.removeEventListener("error", onError);
      window.clearTimeout(timer);
      resolve(value);
    };
    const onMessage = (event: MessageEvent) => {
      if (event.data?.id !== id) return;
      if (event.data?.palette) {
        finish({ palette: event.data.palette as AdaptivePalette, ready: Boolean(event.data.ready) });
      } else {
        finish(null);
      }
    };
    const onError = () => {
      paletteWorkerUnavailable = true;
      finish(null);
    };
    // A hung worker must never pin `sampling` forever.
    const timer = window.setTimeout(() => finish(null), 4000);
    worker.addEventListener("message", onMessage);
    worker.addEventListener("error", onError);
    const transfer = frames.map((frame) => frame.pixels.buffer as ArrayBuffer);
    try {
      worker.postMessage({ id, frames, source, options }, transfer);
    } catch {
      finish(null);
    }
  });
}

function activeMode(): AdaptiveMode {
  return themeMode() ?? "dark";
}

function drawRegion(
  source: HTMLVideoElement | HTMLImageElement,
  sourceWidth: number,
  sourceHeight: number,
  region?: { x: number; y: number; width: number; height: number },
  weight = 1
): FrameSample | null {
  if (!sourceWidth || !sourceHeight) return null;
  const canvas = document.createElement("canvas");
  canvas.width = SAMPLE_EDGE;
  canvas.height = SAMPLE_EDGE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  const sx = region?.x ?? 0;
  const sy = region?.y ?? 0;
  const sw = region?.width ?? sourceWidth;
  const sh = region?.height ?? sourceHeight;
  try {
    ctx.drawImage(source, sx, sy, sw, sh, 0, 0, SAMPLE_EDGE, SAMPLE_EDGE);
    const image = ctx.getImageData(0, 0, SAMPLE_EDGE, SAMPLE_EDGE);
    let min = 255;
    let max = 0;
    for (let i = 0; i < image.data.length; i += 4) {
      const lum =
        image.data[i] * 0.2126 + image.data[i + 1] * 0.7152 + image.data[i + 2] * 0.0722;
      if (lum < min) min = lum;
      if (lum > max) max = lum;
    }
    // A not-yet-decoded frame is uniformly transparent/black; retry later.
    if (max - min < 3) return null;
    return {
      pixels: new Uint8ClampedArray(image.data),
      width: SAMPLE_EDGE,
      height: SAMPLE_EDGE,
      weight,
    };
  } catch {
    // Cross-origin canvas (SecurityError) or a broken decoder.
    return null;
  }
}

function imageFrames(image: HTMLImageElement): FrameSample[] {
  const width = image.naturalWidth;
  const height = image.naturalHeight;
  if (!width || !height) return [];
  const regions = [
    { x: 0, y: 0, width, height, weight: 1.35 },
    { x: width * 0.19, y: height * 0.16, width: width * 0.62, height: height * 0.68, weight: 1.15 },
    { x: 0, y: 0, width: width * 0.56, height: height * 0.56, weight: 0.55 },
    { x: width * 0.44, y: height * 0.44, width: width * 0.56, height: height * 0.56, weight: 0.55 },
  ];
  const frames: FrameSample[] = [];
  for (const region of regions) {
    const frame = drawRegion(image, width, height, region, region.weight);
    if (frame) frames.push(frame);
  }
  return frames;
}

function videoFrames(video: HTMLVideoElement): FrameSample[] {
  if (video.readyState < 2 || !video.videoWidth) return [];
  const frame = drawRegion(video, video.videoWidth, video.videoHeight, undefined, 1.2);
  return frame ? [frame] : [];
}

/**
 * Sample exactly one frame from the already-playing wallpaper video.
 *
 * The previous implementation polled five frames across the timeline, keeping
 * the live decoder busy for ~2 s and delaying palette application. One
 * downsampled frame is representative enough and costs a single read.
 */
function liveVideoFrame(video: HTMLVideoElement): FrameSample[] {
  if (video.paused || video.readyState < 2 || !video.videoWidth) return [];
  return videoFrames(video);
}

function waitForImage(image: HTMLImageElement, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    if (image.complete && image.naturalWidth > 0) return resolve();
    const timer = window.setTimeout(() => reject(new Error("image timeout")), timeoutMs);
    image.addEventListener(
      "load",
      () => {
        window.clearTimeout(timer);
        resolve();
      },
      { once: true }
    );
    image.addEventListener(
      "error",
      () => {
        window.clearTimeout(timer);
        reject(new Error("image error"));
      },
      { once: true }
    );
  });
}

async function sampleCorsImage(src: string): Promise<FrameSample[]> {
  if (!src) return [];
  const image = new Image();
  image.crossOrigin = "anonymous";
  image.decoding = "async";
  image.src = src;
  try {
    await waitForImage(image, MEDIA_TIMEOUT_MS);
    return imageFrames(image);
  } catch {
    return [];
  } finally {
    image.removeAttribute("src");
  }
}

async function sampleBackground(): Promise<{ source: AdaptiveSource; frames: FrameSample[] } | null> {
  const userImage = document.querySelector<HTMLImageElement>(".app-image-layer img");
  if (userImage) {
    let frames = imageFrames(userImage);
    if (frames.length === 0) {
      frames = await sampleCorsImage(userImage.currentSrc || userImage.src);
    }
    if (frames.length > 0) return { source: "image", frames };
  }

  const userVideo = document.querySelector<HTMLVideoElement>(".app-video-layer video");
  if (userVideo) {
    const frames = liveVideoFrame(userVideo);
    if (frames.length > 0) return { source: "video", frames };
  }

  const themeVideo = document.querySelector<HTMLVideoElement>(".theme-media-video");
  if (themeVideo) {
    const frames = liveVideoFrame(themeVideo);
    if (frames.length > 0) return { source: "video", frames };
  }

  const themeImage = document.querySelector<HTMLImageElement>(".theme-media-image, .theme-media-poster");
  if (themeImage) {
    let frames = imageFrames(themeImage);
    if (frames.length === 0) {
      frames = await sampleCorsImage(themeImage.currentSrc || themeImage.src);
    }
    if (frames.length > 0) return { source: "image", frames };
  }
  return null;
}

async function sampleWithRetry(attempts = 4): Promise<{ source: AdaptiveSource; frames: FrameSample[] } | null> {
  const sample = await sampleBackground();
  if (sample) return sample;
  const pendingImage = document.querySelector<HTMLImageElement>(
    ".app-image-layer img, .theme-media-image, .theme-media-poster"
  );
  const pendingVideo = document.querySelector<HTMLVideoElement>(".app-video-layer video, .theme-media-video");
  const imageLoading = Boolean(pendingImage && !pendingImage.complete);
  const videoLoading = Boolean(pendingVideo && pendingVideo.readyState < 2);
  if (attempts > 0 && (imageLoading || videoLoading)) {
    await new Promise((resolve) => window.setTimeout(resolve, 420));
    return sampleWithRetry(attempts - 1);
  }
  return null;
}

function fallbackForCurrentTheme(): AdaptivePalette {
  const tokens = currentThemeTokens();
  return paletteFromThemeAccent(tokens?.accent, activeMode());
}

export async function refreshAdaptivePalette(): Promise<void> {
  if (!enabled()) return;
  if (sampling) {
    await sampling;
    return;
  }
  const mediaKey = currentMediaKey();
  if (mediaKey) {
    const cached = sampledKeys.get(mediaKey);
    if (cached) {
      setPaletteSignal(cached);
      applyPalette(cached);
      setStatusSignal(cached.source === "default" ? "fallback" : "ready");
      return;
    }
  }
  setStatusSignal("sampling");
  sampling = (async () => {
    try {
      const sample = await sampleWithRetry();
      if (!sample) {
        const next = fallbackForCurrentTheme();
        setPaletteSignal(next);
        storePalette(next);
        applyPalette(next);
        setStatusSignal("fallback");
        report(`no media sample; using theme fallback ${next.colors.join(" ")}`, "warn");
        return;
      }
      const sampleCount = sample.frames.reduce(
        (sum, frame) => sum + Math.floor((frame.width * frame.height) / 4),
        0
      );
      const options = {
        mode: activeMode(),
        frameCount: sample.frames.length,
        sampleCount,
      };
      let next: AdaptivePalette;
      let ready: boolean;
      if (getPaletteWorker()) {
        // Buffers are transferred to the worker once; do not re-read them here.
        const result = await computePaletteInWorker(sample.frames, sample.source, options);
        if (result) {
          next = result.palette;
          ready = result.ready;
        } else {
          next = fallbackForCurrentTheme();
          ready = false;
        }
      } else {
        const clusters = extractClustersFromFrames(sample.frames, 6);
        next = buildAdaptivePalette(clusters, sample.source, options);
        ready = clusters.length > 0;
      }
      setPaletteSignal(next);
      storePalette(next);
      applyPalette(next);
      setStatusSignal(ready ? "ready" : "fallback");
      if (mediaKey && ready) sampledKeys.set(mediaKey, next);
      report(
        `sampled ${sample.source} frames=${sample.frames.length} samples=${sampleCount} ` +
          `mode=${next.mode} colors=${next.colors.join(" ")} confidence=${next.confidence.toFixed(2)} ` +
          `readable=${next.readable}`,
        next.readable ? "info" : "warn"
      );
    } catch (error) {
      const next = fallbackForCurrentTheme();
      setPaletteSignal(next);
      storePalette(next);
      applyPalette(next);
      setStatusSignal("fallback");
      report(`sampling failed: ${String(error)}`, "warn");
    } finally {
      sampling = null;
    }
  })();
  await sampling;
}

export function scheduleAdaptiveSample(delay = 450): void {
  if (!enabled()) return;
  if (retryTimer) clearTimeout(retryTimer);
  retryTimer = setTimeout(() => {
    retryTimer = null;
    void refreshAdaptivePalette();
  }, delay);
}

export async function enableAdaptive(): Promise<void> {
  if (!enabled()) {
    setEnabledSignal(true);
    storeEnabled(true);
  }
  const current = palette() ?? fallbackForCurrentTheme();
  setPaletteSignal(current);
  applyPalette(current);
  setStatusSignal("sampling");
  await refreshAdaptivePalette();
}

export async function disableAdaptive(restoreTheme = true): Promise<void> {
  setEnabledSignal(false);
  storeEnabled(false);
  clearAdaptiveVars();
  setStatusSignal("idle");
  // Adaptive also overwrites the base --rmccc-* variables (not just
  // --adaptive-*). Clearing it therefore has to reapply the active theme
  // tokens synchronously; otherwise the shell loses its background/text
  // variables and turns into a transparent black-on-white layer.
  const tokens = currentThemeTokens();
  if (tokens) applyThemeTokens(tokens);
  if (restoreTheme) {
    try {
      await applyThemePreference(getThemePreference());
    } catch {
      // The cached tokens above keep the shell readable if the backend fails.
    }
  }
}

/** Called once from the app shell after initTheme(). */
export async function initAdaptiveTheme(): Promise<void> {
  if (!enabled()) return;
  try {
    const mode = activeMode();
    const previous = palette();
    const stored = previous && previous.mode === mode ? previous : fallbackForCurrentTheme();
    setPaletteSignal(stored);
    applyPalette(stored);
    setStatusSignal(stored.source === "default" ? "fallback" : "ready");
    scheduleAdaptiveSample(900);
  } catch (error) {
    clearAdaptiveVars();
    setStatusSignal("idle");
    report(`initialisation failed: ${String(error)}`, "warn");
  }
}
