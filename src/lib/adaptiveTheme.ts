/**
 * Adaptive appearance mode.
 *
 * Samples the visible wallpaper (user MP4 first, then theme image/video),
 * extracts 2–3 dominant colours and applies a contrast-checked palette to the
 * whole shell: tokens for non-home pages, soft-dashboard variables for cards,
 * docks, buttons, highlights and status colours.
 *
 * Canvas sampling is best-effort: a cross-origin video may taint the canvas, in
 * which case a hidden CORS clone is tried before falling back to the current
 * theme accent. The user can re-sample at any time.
 */
import { createRoot, createSignal } from "solid-js";
import {
  buildAdaptivePalette,
  extractClusters,
  paletteFromThemeAccent,
  type AdaptivePalette,
  type AdaptiveSource,
  type RawCluster,
} from "./adaptivePalette";
import { applyThemePreference, currentThemeTokens, getThemePreference } from "../theme";
import { reportFrontendMessage } from "../ipc/api";

const ENABLED_KEY = "rmccc.adaptiveTheme.v1";
const PALETTE_KEY = "rmccc.adaptivePalette.v1";
const SAMPLE_SIZE = 64;
const CORS_TIMEOUT_MS = 2800;

export type AdaptiveStatus = "idle" | "sampling" | "ready" | "fallback";

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
      parsed?.version !== 1 ||
      !Array.isArray(parsed.colors) ||
      parsed.colors.length < 2 ||
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
}

function safeColorVars(): Record<string, string> {
  return {
    "--color-red": "#c0392b",
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
    // New token namespace (visual-system / non-home pages).
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
    // Legacy bridge consumed by app.css.
    "--surface-bg": canvas,
    "--surface-card": panel,
    "--surface-raised": raised,
    "--surface-input": raised,
    "--surface-hover": v["--adaptive-chip"],
    "--text-primary": ink,
    "--text-secondary": muted,
    "--text-on-input": ink,
    "--color-blue": accent,
    "--border-default": line,
    "--border-subtle": line,
    // Soft dashboard palette (Home).
    "--soft-canvas": canvas,
    "--soft-panel": panel,
    "--soft-panel-raised": raised,
    "--soft-ink": ink,
    "--soft-muted": muted,
    "--soft-line": line,
    "--soft-blue": accent,
    "--soft-blue-deep": secondary,
    "--soft-green": secondary,
    "--soft-yellow": v["--adaptive-badge"],
  };
}

function applyPalette(p: AdaptivePalette): void {
  clearAdaptiveVars();
  for (const [name, value] of Object.entries(p.vars)) setRootVar(name, value);
  for (const [name, value] of Object.entries(tokenVars(p))) setRootVar(name, value);
  for (const [name, value] of Object.entries(safeColorVars())) setRootVar(name, value);
  const root = document.documentElement;
  root.dataset.adaptive = "on";
  root.dataset.themeMode = "light";
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

interface FrameSample {
  data: Uint8ClampedArray;
  width: number;
  height: number;
}

function drawFrame(
  source: HTMLVideoElement | HTMLImageElement,
  sw: number,
  sh: number
): FrameSample | null {
  if (!sw || !sh) return null;
  const canvas = document.createElement("canvas");
  canvas.width = SAMPLE_SIZE;
  canvas.height = SAMPLE_SIZE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  const scale = Math.max(SAMPLE_SIZE / sw, SAMPLE_SIZE / sh);
  const width = sw * scale;
  const height = sh * scale;
  const dx = (SAMPLE_SIZE - width) / 2;
  const dy = (SAMPLE_SIZE - height) / 2;
  try {
    ctx.drawImage(source, dx, dy, width, height);
    const image = ctx.getImageData(0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
    // A not-yet-decoded video frame is uniformly transparent/black; retry later.
    let min = 255;
    let max = 0;
    for (let i = 0; i < image.data.length; i += 4) {
      const lum = image.data[i] * 0.2126 + image.data[i + 1] * 0.7152 + image.data[i + 2] * 0.0722;
      if (lum < min) min = lum;
      if (lum > max) max = lum;
    }
    if (max - min < 3) return null;
    return { data: new Uint8ClampedArray(image.data), width: SAMPLE_SIZE, height: SAMPLE_SIZE };
  } catch {
    // Cross-origin canvas (SecurityError) or a broken decoder.
    return null;
  }
}

function waitForMedia(element: HTMLVideoElement | HTMLImageElement, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    if (element instanceof HTMLImageElement) {
      if (element.complete && element.naturalWidth > 0) return resolve();
      element.addEventListener("load", () => resolve(), { once: true });
      element.addEventListener("error", () => reject(new Error("image error")), { once: true });
    } else {
      if (element.readyState >= 2 && element.videoWidth > 0) return resolve();
      element.addEventListener("loadeddata", () => resolve(), { once: true });
      element.addEventListener("error", () => reject(new Error("video error")), { once: true });
    }
    window.setTimeout(() => reject(new Error("media timeout")), timeoutMs);
  });
}

async function sampleCorsVideo(src: string): Promise<FrameSample | null> {
  if (!src) return null;
  const video = document.createElement("video");
  video.crossOrigin = "anonymous";
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  video.style.cssText = "position:fixed;left:-9999px;top:-9999px;width:2px;height:2px;opacity:0;pointer-events:none;";
  video.src = src;
  document.body.appendChild(video);
  try {
    await waitForMedia(video, CORS_TIMEOUT_MS);
    if (video.readyState < 2) video.currentTime = 0.001;
    return drawFrame(video, video.videoWidth, video.videoHeight);
  } catch {
    return null;
  } finally {
    video.removeAttribute("src");
    video.load();
    video.remove();
  }
}

async function sampleCorsImage(src: string): Promise<FrameSample | null> {
  if (!src) return null;
  const image = new Image();
  image.crossOrigin = "anonymous";
  image.decoding = "async";
  image.src = src;
  try {
    await waitForMedia(image, CORS_TIMEOUT_MS);
    return drawFrame(image, image.naturalWidth, image.naturalHeight);
  } catch {
    return null;
  }
}

function sampleVideoElement(video: HTMLVideoElement): FrameSample | null {
  if (video.readyState < 2 || !video.videoWidth) return null;
  return drawFrame(video, video.videoWidth, video.videoHeight);
}

function sampleImageElement(image: HTMLImageElement): FrameSample | null {
  if (!image.complete || !image.naturalWidth) return null;
  return drawFrame(image, image.naturalWidth, image.naturalHeight);
}

async function sampleBackground(): Promise<{ source: AdaptiveSource; clusters: RawCluster[] } | null> {
  const userVideo = document.querySelector<HTMLVideoElement>(".app-video-layer video");
  if (userVideo) {
    let frame = sampleVideoElement(userVideo);
    if (!frame) frame = await sampleCorsVideo(userVideo.currentSrc || userVideo.src);
    if (frame) {
      const clusters = extractClusters(frame.data, frame.width, frame.height);
      if (clusters.length > 0) return { source: "video", clusters };
    }
  }

  const themeVideo = document.querySelector<HTMLVideoElement>(".theme-media-video");
  if (themeVideo) {
    let frame = sampleVideoElement(themeVideo);
    if (!frame) frame = await sampleCorsVideo(themeVideo.currentSrc || themeVideo.src);
    if (frame) {
      const clusters = extractClusters(frame.data, frame.width, frame.height);
      if (clusters.length > 0) return { source: "video", clusters };
    }
  }

  const themeImage = document.querySelector<HTMLImageElement>(".theme-media-image, .theme-media-poster");
  if (themeImage) {
    let frame = sampleImageElement(themeImage);
    if (!frame) frame = await sampleCorsImage(themeImage.currentSrc || themeImage.src);
    if (frame) {
      const clusters = extractClusters(frame.data, frame.width, frame.height);
      if (clusters.length > 0) return { source: "image", clusters };
    }
  }
  return null;
}

async function sampleWithRetry(attempts = 3): Promise<{ source: AdaptiveSource; clusters: RawCluster[] } | null> {
  const sample = await sampleBackground();
  if (sample) return sample;
  const pending = document.querySelector<HTMLVideoElement>(".app-video-layer video, .theme-media-video");
  if (attempts > 0 && pending && pending.readyState < 2) {
    await new Promise((resolve) => window.setTimeout(resolve, 500));
    return sampleWithRetry(attempts - 1);
  }
  return null;
}

function fallbackForCurrentTheme(source: AdaptiveSource = "default"): AdaptivePalette {
  const tokens = currentThemeTokens();
  return tokens?.accent ? paletteFromThemeAccent(tokens.accent) : paletteFromThemeAccent("#6f91ca");
}

export async function refreshAdaptivePalette(): Promise<void> {
  if (!enabled()) return;
  if (sampling) {
    await sampling;
    return;
  }
  setStatusSignal("sampling");
  sampling = (async () => {
    try {
      const sample = await sampleWithRetry();
      const next = sample
        ? buildAdaptivePalette(sample.clusters, sample.source)
        : fallbackForCurrentTheme("default");
      setPaletteSignal(next);
      storePalette(next);
      applyPalette(next);
      setStatusSignal(sample ? "ready" : "fallback");
      report(
        sample
          ? `sampled ${sample.source} ${next.colors.join(" ")} readable=${next.readable}`
          : `no media sample; using theme fallback ${next.colors.join(" ")}`,
        sample ? "info" : "warn"
      );
    } catch (error) {
      const next = fallbackForCurrentTheme("default");
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
  if (restoreTheme) {
    try {
      await applyThemePreference(getThemePreference());
    } catch {
      // The last theme remains in the DOM if the backend is unavailable.
    }
  }
}

/** Called once from the app shell after initTheme(). */
export async function initAdaptiveTheme(): Promise<void> {
  if (!enabled()) return;
  try {
    const stored = palette() ?? fallbackForCurrentTheme();
    setPaletteSignal(stored);
    applyPalette(stored);
    setStatusSignal(stored.source === "default" ? "fallback" : "ready");
    scheduleAdaptiveSample(900);
  } catch (error) {
    // Adaptive colouring must never keep the launcher from booting.
    clearAdaptiveVars();
    setStatusSignal("idle");
    report(`initialisation failed: ${String(error)}`, "warn");
  }
}
