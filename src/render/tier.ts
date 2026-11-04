/**
 * Render tier engine (ui-render-refactor S1 / `02_RENDER_TIER_ENGINE.md`).
 *
 * Single source of truth for "how expensive may the UI look right now?".
 * Everything else (backdrop, surfaces, motion, theme effects) reads the tier
 * instead of sniffing the platform or stacked `data-*` flags on its own.
 *
 * Tiers:
 *  - `full`    — native video off-screen already; real backdrop-filter allowed
 *                over static backdrops (Windows/Chromium).
 *  - `acrylic` — default; glass surfaces are tinted/faux glass and never sample
 *                the live backdrop (macOS, or any moving video wallpaper).
 *  - `static`  — reduced motion / user motion off / low-end GPU: no ambient
 *                motion, opaque surfaces.
 *
 * The tier is published as `<html data-render-tier>`; the active backdrop as
 * `<html data-backdrop>`. These two attributes replace the previous ad-hoc
 * `data-video-wallpaper` / `data-image-wallpaper` selectors.
 */
import { createEffect, createMemo, createRoot, createSignal } from "solid-js";
import { settingsStore } from "../stores/settings";
import { decideTier } from "./tier-decision";
import type { BackdropKind, GpuTier, PlatformKind, RenderTier } from "./tier-decision";

export { decideTier };
export type { BackdropKind, GpuTier, PlatformKind, RenderTier, TierInputs } from "./tier-decision";

const GPU_KEY = "rmccc.gpuTier.v2";

function readCachedGpu(): GpuTier | null {
  try {
    const raw = localStorage.getItem(GPU_KEY);
    return raw === "low" || raw === "standard" || raw === "high" ? raw : null;
  } catch {
    return null;
  }
}

/**
 * Conservative one-shot probe; failure always maps to `standard`.
 *
 * `high` is reserved for discrete GPUs (NVIDIA/AMD/Intel Arc). Integrated GPUs
 * stay `standard` so Windows does not default to 70 live `backdrop-filter`
 * layers on a laptop iGPU that was never measured.
 */
function detectGpuTier(): GpuTier {
  if (typeof document === "undefined") return "standard";
  const cached = readCachedGpu();
  if (cached) return cached;
  let tier: GpuTier = "standard";
  try {
    const canvas = document.createElement("canvas");
    const gl =
      (canvas.getContext("webgl") as WebGLRenderingContext | null) ??
      (canvas.getContext("experimental-webgl") as WebGLRenderingContext | null);
    if (!gl) {
      tier = "low";
    } else {
      const info = gl.getExtension("WEBGL_debug_renderer_info");
      const renderer = info
        ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL) ?? "")
        : "";
      if (/swiftshader|llvmpipe|software|basic render/i.test(renderer)) {
        tier = "low";
      } else if (
        /nvidia|geforce|rtx|gtx|quadro|radeon rx|rx \d{3,4}|arc a\d/i.test(renderer)
      ) {
        tier = "high";
      }
      const lose = gl.getExtension("WEBGL_lose_context");
      lose?.loseContext();
    }
  } catch {
    tier = "standard";
  }
  try {
    localStorage.setItem(GPU_KEY, tier);
  } catch {
    /* persistence is best-effort */
  }
  return tier;
}

function detectPlatform(): PlatformKind {
  if (typeof navigator === "undefined") return "other";
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  return "other";
}

const [platformSignal] = createRoot(() => createSignal<PlatformKind>(detectPlatform()));
const [gpuSignal] = createRoot(() => createSignal<GpuTier>(detectGpuTier()));
const [backdropSignal, setBackdropSignal] = createRoot(() =>
  createSignal<BackdropKind>("ambient")
);
const [reducedSignal, setReducedSignal] = createRoot(() => createSignal(false));

/** Reactive tier — recomputes when platform, GPU, motion or backdrop changes. */
const tierMemo = createRoot(() =>
  createMemo(() =>
    decideTier({
      platform: platformSignal(),
      reducedMotion: reducedSignal(),
      userMotion: settingsStore.motionEnabled(),
      gpuTier: gpuSignal(),
      backdropKind: backdropSignal(),
    })
  )
);

let initialized = false;
let mediaQuery: MediaQueryList | null = null;
let mediaListener: ((event: MediaQueryListEvent) => void) | null = null;

function publish(): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.renderTier = tierMemo();
  root.dataset.backdrop = backdropSignal();
}

// Keep the published attributes in sync with every reactive input (motion
// toggle, backdrop change, reduced-motion change). Without this effect a
// settings toggle would update `data-motion` but leave `data-render-tier`
// stale, so glass surfaces would not re-tier until the next restart.
createRoot(() => {
  createEffect(() => {
    // Touch the inputs so the effect tracks them, then publish.
    tierMemo();
    backdropSignal();
    publish();
  });
});

/** Current tier (reactive). */
export function renderTier(): RenderTier {
  return tierMemo();
}

/** Current platform family (reactive). */
export function platformKind(): PlatformKind {
  return platformSignal();
}

/** Current backdrop kind (reactive). */
export function backdropKind(): BackdropKind {
  return backdropSignal();
}

/** Called by the backdrop subsystem when the active backdrop changes. */
export function setBackdropKind(kind: BackdropKind): void {
  if (backdropSignal() === kind) return;
  setBackdropSignal(kind);
  publish();
}

/**
 * Initialize once from the app shell. Safe to call repeatedly; returns a
 * disposer suitable for `onCleanup`.
 */
export function initRenderTier(): () => void {
  if (typeof window === "undefined") return () => {};
  if (!initialized) {
    initialized = true;
    mediaQuery = window.matchMedia?.("(prefers-reduced-motion: reduce)") ?? null;
    if (mediaQuery) {
      setReducedSignal(mediaQuery.matches);
      mediaListener = (event) => {
        setReducedSignal(event.matches);
        publish();
      };
      mediaQuery.addEventListener("change", mediaListener);
    }
    // The GPU probe is cached; a fresh platform/motion state still needs a repaint.
    publish();
  }
  return () => {
    if (mediaQuery && mediaListener) {
      mediaQuery.removeEventListener("change", mediaListener);
      mediaListener = null;
    }
  };
}
