/**
 * Appearance glass controls (soft-skeuo dashboard).
 *
 * Two independent, per-machine preferences:
 *  - `panelOpacity`: the frame behind non-home pages (`--soft-frame-opacity`);
 *    0 % means the page shows the wallpaper/video completely.
 *  - `cardOpacity`: the background of the home cards (`--soft-card-opacity`).
 *
 * Both are persisted in localStorage (visual preferences, not launcher config)
 * and applied as CSS variables on <html>.
 */
import { createRoot, createSignal } from "solid-js";

const PANEL_KEY = "rmccc.panelOpacity.v1";
const CARD_KEY = "rmccc.cardOpacity.v1";
const DEFAULT_PANEL = 88;
const DEFAULT_CARD = 68;

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

function clampPercent(value: number): number {
  return Math.round(Math.min(100, Math.max(0, value)));
}

const [opacity, setOpacitySignal] = createRoot(() => createSignal(readPercent(PANEL_KEY, DEFAULT_PANEL)));
const [cardOpacity, setCardOpacitySignal] = createRoot(() => createSignal(readPercent(CARD_KEY, DEFAULT_CARD)));

function applyPanelOpacity(value: number): void {
  const root = document.documentElement;
  root.style.setProperty("--soft-frame-opacity", String(value / 100));
  // 0 % must be genuinely transparent: drop the 1 px border, shadow and
  // backdrop blur instead of leaving a faint frame around the content.
  if (value === 0) root.dataset.panelTransparent = "on";
  else delete root.dataset.panelTransparent;
}

type Rgba = [number, number, number, number];

function mixRgba(from: Rgba, to: Rgba, t: number): string {
  const channel = (index: number) => Math.round(from[index] + (to[index] - from[index]) * t);
  const alpha = from[3] + (to[3] - from[3]) * t;
  return `rgba(${channel(0)}, ${channel(1)}, ${channel(2)}, ${alpha.toFixed(3)})`;
}

/**
 * The blue hero card switches from white ink to dark ink as its glass fades,
 * so the launch content stays readable at 0 %.  The blend only starts below
 * the design default (68 %), keeping the normal look byte-identical.
 */
function applyLaunchInk(value: number): void {
  const blend = Math.min(1, Math.max(0, (0.68 - value / 100) / 0.68));
  const root = document.documentElement;
  root.style.setProperty("--soft-launch-ink", mixRgba([248, 251, 255, 1], [38, 51, 58, 1], blend));
  root.style.setProperty(
    "--soft-launch-muted",
    mixRgba([249, 252, 255, 0.72], [70, 84, 90, 1], blend)
  );
}

function applyCardOpacity(value: number): void {
  const root = document.documentElement;
  root.style.setProperty("--soft-card-opacity", String(value / 100));
  applyLaunchInk(value);
  if (value === 0) root.dataset.cardsTransparent = "on";
  else delete root.dataset.cardsTransparent;
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

applyPanelOpacity(opacity());
applyCardOpacity(cardOpacity());
