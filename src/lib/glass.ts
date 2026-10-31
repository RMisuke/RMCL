/**
 * Appearance glass controls.
 *
 * Three independent, per-machine preferences persisted in localStorage (visual
 * preferences, not launcher config) and applied as CSS variables on <html>:
 *
 *  - `panelOpacity`: the shell behind non-home pages (`--rmccc-panel-alpha`);
 *    0 % means the wallpaper/video shows through completely.
 *  - `cardOpacity`: the background of cards (`--rmccc-card-alpha`); 0 % makes
 *    cards genuinely transparent, including their border/shadow/blur.
 *  - `cardBlur`: the backdrop-filter radius for cards (`--rmccc-card-blur`).
 *    Home cards use half of it while an image/video wallpaper is active, so the
 *    wallpaper stays readable without turning every surface into a solid slab.
 *
 * The legacy `--soft-*` variables are still published for third-party skin CSS
 * written against the previous dashboard, but the host consumes the
 * `--rmccc-*` variables exclusively.
 */
import { createRoot, createSignal } from "solid-js";

const PANEL_KEY = "rmccc.panelOpacity.v1";
const CARD_KEY = "rmccc.cardOpacity.v1";
const CARD_BLUR_KEY = "rmccc.cardBlur.v1";
const DEFAULT_PANEL = 74;
const DEFAULT_CARD = 62;
const DEFAULT_CARD_BLUR = 24;
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

function applyPanelOpacity(value: number): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty("--rmccc-panel-alpha", `${value}%`);
  root.style.setProperty("--soft-frame-opacity", String(value / 100));
  // 0 % must be genuinely transparent: drop the 1 px border, shadow and
  // backdrop blur instead of leaving a faint frame around the content.
  if (value === 0) root.dataset.panelTransparent = "on";
  else delete root.dataset.panelTransparent;
}

function applyCardOpacity(value: number): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty("--rmccc-card-alpha", `${value}%`);
  root.style.setProperty("--soft-card-opacity", String(value / 100));
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
  applyPanelOpacity(next);
  try {
    localStorage.setItem(PANEL_KEY, String(next));
  } catch {
    // The live preview still works when persistent storage is unavailable.
  }
}

export function homeCardOpacity(): number {
  return cardOpacity();
}

export function setHomeCardOpacity(value: number): void {
  const next = clampPercent(value);
  setCardOpacitySignal(next);
  applyCardOpacity(next);
  try {
    localStorage.setItem(CARD_KEY, String(next));
  } catch {
    // Same as above.
  }
}

export function homeCardBlur(): number {
  return cardBlur();
}

export function setHomeCardBlur(value: number): void {
  const next = clampBlur(value);
  setCardBlurSignal(next);
  applyCardBlur(next);
  try {
    localStorage.setItem(CARD_BLUR_KEY, String(next));
  } catch {
    // Same as above.
  }
}

applyPanelOpacity(opacity());
applyCardOpacity(cardOpacity());
applyCardBlur(cardBlur());
