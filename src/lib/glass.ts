/**
 * Appearance glass controls.
 *
 * Three independent, per-machine preferences persisted in localStorage (visual
 * preferences, not launcher config) and applied as CSS variables on <html>:
 *
 *  - `panelOpacity`: the shell behind non-home pages (`--rmccc-panel-alpha`)
 *    plus the always-visible dock chrome (`--rmccc-dock-alpha`); 0 % means the
 *    wallpaper/video shows through completely.
 *  - `cardOpacity`: the background of cards (`--rmccc-card-alpha`); 0 % makes
 *    cards genuinely transparent, including their border/shadow/blur.
 *  - `cardBlur`: the backdrop-filter radius for cards (`--rmccc-card-blur`).
 *    Home cards use half of it while an image/video wallpaper is active, so the
 *    wallpaper stays readable without turning every surface into a solid slab.
 */
import { createRoot, createSignal } from "solid-js";

const PANEL_KEY = "rmccc.panelOpacity.v1";
const CARD_KEY = "rmccc.cardOpacity.v1";
const CARD_BLUR_KEY = "rmccc.cardBlur.v1";
const DEFAULT_PANEL = 74;
const DEFAULT_CARD = 62;
// Enough to keep the frosted edge readable without forcing every card to
// recompute a 24 px backdrop over the animated ambient stage.
const DEFAULT_CARD_BLUR = 16;
const MAX_CARD_BLUR = 40;

function readPercent(key: string, fallback: number): number {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const value = Number(raw);
    return Number.isFinite(value) && value >= 0 && value <= 100 ? value : fallback;
  } catch {
    return fallback;
  }
}

function readBlur(): number {
  try {
    const raw = localStorage.getItem(CARD_BLUR_KEY);
    if (raw === null) return DEFAULT_CARD_BLUR;
    const value = Number(raw);
    return Number.isFinite(value)
      ? Math.round(Math.min(MAX_CARD_BLUR, Math.max(0, value)))
      : DEFAULT_CARD_BLUR;
  } catch {
    return DEFAULT_CARD_BLUR;
  }
}

function clampPercent(value: number): number {
  return Math.round(Math.min(100, Math.max(0, value)));
}

function clampBlur(value: number): number {
  return Math.round(Math.min(MAX_CARD_BLUR, Math.max(0, value)));
}

const [opacity, setOpacitySignal] = createRoot(() => createSignal(readPercent(PANEL_KEY, DEFAULT_PANEL)));
const [cardOpacity, setCardOpacitySignal] = createRoot(() => createSignal(readPercent(CARD_KEY, DEFAULT_CARD)));
const [cardBlur, setCardBlurSignal] = createRoot(() => createSignal(readBlur()));

// Drag-time smoothness: applying the CSS variable and writing localStorage on
// every `input` event did heavy full-window glass repaints and a synchronous
// storage write per pixel of travel, so the slider visibly chased the thumb in
// steps. Coalesce the CSS write to one per animation frame (always reading the
// latest value) and debounce persistence; the slider/output still update
// synchronously, only the expensive side is throttled.
const rafHandles = new Map<string, number>();
const persistTimers = new Map<string, ReturnType<typeof setTimeout>>();

function scheduleApply(key: string, apply: () => void): void {
  if (typeof requestAnimationFrame === "undefined") {
    apply();
    return;
  }
  if (rafHandles.has(key)) return;
  const handle = requestAnimationFrame(() => {
    rafHandles.delete(key);
    apply();
  });
  rafHandles.set(key, handle);
}

function schedulePersist(key: string, value: number): void {
  const existing = persistTimers.get(key);
  if (existing) clearTimeout(existing);
  persistTimers.set(
    key,
    setTimeout(() => {
      persistTimers.delete(key);
      try {
        localStorage.setItem(key, String(value));
      } catch {
        // The live preview still works when persistent storage is unavailable.
      }
    }, 200)
  );
}

function applyPanelOpacity(value: number): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty("--rmccc-panel-alpha", `${value}%`);
  // The docks share this slider instead of owning a fixed white fill. The
  // scale keeps 100 % at a light 0.20 alpha, so even the maximum stays glass.
  root.style.setProperty("--rmccc-dock-alpha", (value / 500).toFixed(3));
  // 0 % must be genuinely transparent: drop the 1 px border, shadow and
  // backdrop blur instead of leaving a faint frame around the content.
  if (value === 0) root.dataset.panelTransparent = "on";
  else delete root.dataset.panelTransparent;
}

function applyCardOpacity(value: number): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty("--rmccc-card-alpha", `${value}%`);
  if (value === 0) root.dataset.cardsTransparent = "on";
  else delete root.dataset.cardsTransparent;
}

function applyCardBlur(value: number): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.setProperty("--rmccc-card-blur", `${value}px`);
}

export function panelOpacity(): number {
  return opacity();
}

export function setPanelOpacity(value: number): void {
  const next = clampPercent(value);
  setOpacitySignal(next);
  scheduleApply("panel", () => applyPanelOpacity(opacity()));
  schedulePersist(PANEL_KEY, next);
}

export function homeCardOpacity(): number {
  return cardOpacity();
}

export function setHomeCardOpacity(value: number): void {
  const next = clampPercent(value);
  setCardOpacitySignal(next);
  scheduleApply("card", () => applyCardOpacity(cardOpacity()));
  schedulePersist(CARD_KEY, next);
}

export function homeCardBlur(): number {
  return cardBlur();
}

export function setHomeCardBlur(value: number): void {
  const next = clampBlur(value);
  setCardBlurSignal(next);
  scheduleApply("blur", () => applyCardBlur(cardBlur()));
  schedulePersist(CARD_BLUR_KEY, next);
}

applyPanelOpacity(opacity());
applyCardOpacity(cardOpacity());
applyCardBlur(cardBlur());
