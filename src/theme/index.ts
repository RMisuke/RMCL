/**
 * Theme token application (Phase 3, ADR-005).
 *
 * The backend owns token validation; this module maps tokens to CSS variables
 * and bridges them to the legacy `--surface-*` / `--text-*` variables so the
 * existing stylesheet keeps working while new widgets use `--rmccc-*`.
 */

import { createSignal } from "solid-js";
import { dashboard } from "../ipc/api";
import type { ThemeInfo, ThemeTokens } from "../ipc/types";

const DARK_FALLBACK: ThemeTokens = {
  background: "#0b1020",
  surface: "#141a2e",
  surfaceElevated: "#1c2440",
  text: "#e6e9f2",
  textMuted: "#8b93a8",
  accent: "#7c6cff",
  accentContrast: "#ffffff",
  border: "#2a3352",
};

const LIGHT_FALLBACK: ThemeTokens = {
  background: "#f2f2f4",
  surface: "#ffffff",
  surfaceElevated: "#fafafa",
  text: "#1a1a2e",
  textMuted: "#888888",
  accent: "#1a56db",
  accentContrast: "#ffffff",
  border: "#e0e0e4",
};

/** Host-owned decorative palette. The public theme schema remains unchanged. */
export interface VisualPalette {
  primary: string;
  secondary: string;
  tertiary: string;
}

const VISUAL_PALETTES: Record<string, VisualPalette> = {
  "builtin:dark": { primary: "#8b7cff", secondary: "#38d9ff", tertiary: "#ff5fa2" },
  "builtin:forest": { primary: "#58d68d", secondary: "#b7e56b", tertiary: "#36c9b0" },
  "builtin:ocean": { primary: "#38bdf8", secondary: "#36e0c2", tertiary: "#7c8cff" },
  "builtin:ember": { primary: "#ff8a4c", secondary: "#ffcf5c", tertiary: "#ff5e7a" },
  "builtin:light": { primary: "#5b5bd6", secondary: "#00a6c7", tertiary: "#ec5b9f" },
  "builtin:sakura": { primary: "#e75d9c", secondary: "#9b72ee", tertiary: "#ff9b71" },
  "builtin:paper": { primary: "#a56a24", secondary: "#357a68", tertiary: "#c45a4a" },
  "builtin:sky": { primary: "#2589e8", secondary: "#5a6ee8", tertiary: "#14b8a6" },
};

export function visualPaletteFor(
  preference: string | null,
  tokens: ThemeTokens,
  mode: ThemeMode = isDark(tokens.background) ? "dark" : "light"
): VisualPalette {
  if (preference && VISUAL_PALETTES[preference]) return VISUAL_PALETTES[preference];
  const fallback = mode === "light" ? "builtin:light" : "builtin:dark";
  return { ...VISUAL_PALETTES[fallback], primary: tokens.accent };
}

/** Effective mode for host-owned safety colors. */
export type ThemeMode = "light" | "dark";

/** Reactive effective mode (N-11): ambient/UI layers re-resolve when the
 *  applied tokens change (system dark/light switch, theme swap). */
const [effectiveMode, setEffectiveMode] = createSignal<ThemeMode | null>(null);
export function themeMode(): ThemeMode | null {
  return effectiveMode();
}

let currentPreference: string | null = null;
let systemMedia: MediaQueryList | null = null;
/// Effective-mode listeners (multi-slot, P3-6).
const modeListeners = new Set<(mode: ThemeMode) => void>();
/// Last applied tokens, so the plugin UI bridge can hand the same palette to
/// isolated frames without re-reading the backend.
let currentTokens: ThemeTokens | null = null;
const tokenListeners = new Set<(tokens: ThemeTokens) => void>();

function isDark(hex: string): boolean {
  const value = hex.replace("#", "");
  if (value.length !== 6 && value.length !== 3) return true;
  const full =
    value.length === 3
      ? value
          .split("")
          .map((c) => c + c)
          .join("")
      : value;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  // Relative luminance (sRGB approximation).
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 < 0.5;
}

function setVar(name: string, value: string) {
  document.documentElement.style.setProperty(name, value);
}

/** Apply theme tokens + host-owned safety colors. */
export function applyThemeTokens(tokens: ThemeTokens): ThemeMode {
  const root = document.documentElement;
  const mode: ThemeMode = isDark(tokens.background) ? "dark" : "light";

  // New token namespace.
  setVar("--rmccc-background", tokens.background);
  setVar("--rmccc-surface", tokens.surface);
  setVar("--rmccc-surface-elevated", tokens.surfaceElevated);
  setVar("--rmccc-text", tokens.text);
  setVar("--rmccc-text-muted", tokens.textMuted);
  setVar("--rmccc-accent", tokens.accent);
  setVar("--rmccc-accent-contrast", tokens.accentContrast);
  setVar("--rmccc-border", tokens.border);

  const palette = visualPaletteFor(currentPreference, tokens, mode);
  setVar("--rmccc-accent-secondary", palette.secondary);
  setVar("--rmccc-accent-tertiary", palette.tertiary);
  setVar("--rmccc-accent-gradient", `linear-gradient(135deg, ${palette.primary}, ${palette.secondary})`);
  root.dataset.visualTheme = currentPreference?.replace("builtin:", "") ?? mode;

  // Bridge to the legacy variables used by app.css.
  setVar("--surface-bg", tokens.background);
  setVar("--surface-card", tokens.surface);
  setVar("--surface-raised", tokens.surfaceElevated);
  setVar("--surface-input", tokens.surfaceElevated);
  setVar("--surface-hover", tokens.surfaceElevated);
  setVar("--text-primary", tokens.text);
  setVar("--text-secondary", tokens.textMuted);
  setVar("--text-on-input", tokens.text);
  setVar("--color-blue", tokens.accent);
  setVar("--border-default", tokens.border);
  setVar("--border-subtle", tokens.border);

  // Host-owned safety colors — never taken from the theme JSON.
  if (mode === "dark") {
    setVar("--color-red", "#ff6b6b");
    setVar("--color-red-light", "rgba(255,107,107,0.12)");
    setVar("--color-red-border", "rgba(255,107,107,0.4)");
    setVar("--color-green", "#4ade80");
    setVar("--color-yellow", "#f5a524");
  } else {
    setVar("--color-red", "#dc2626");
    setVar("--color-red-light", "#fef2f2");
    setVar("--color-red-border", "#fca5a5");
    setVar("--color-green", "#16a34a");
    setVar("--color-yellow", "#d97706");
  }

  root.dataset.themeMode = mode;
  setEffectiveMode(mode);
  currentTokens = tokens;
  for (const listener of tokenListeners) {
    try {
      listener(tokens);
    } catch (e) {
      console.warn("theme: token listener failed", e);
    }
  }
  for (const listener of modeListeners) {
    try {
      listener(mode);
    } catch (e) {
      console.warn("theme: mode listener failed", e);
    }
  }
  return mode;
}

function fallbackFor(mode: ThemeMode): ThemeTokens {
  return mode === "dark" ? DARK_FALLBACK : LIGHT_FALLBACK;
}

function systemMode(): ThemeMode {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

async function resolveTokens(preference: string | null): Promise<ThemeTokens> {
  if (preference === "light") return LIGHT_FALLBACK;
  if (preference === "dark") return DARK_FALLBACK;
  if (preference === "system" || preference === null) {
    return fallbackFor(systemMode());
  }
  const res = await dashboard.getThemeTokens(preference);
  if (!res.ok) {
    console.warn("theme: could not load tokens", res.error);
    return fallbackFor(systemMode());
  }
  return res.data;
}

/** Apply a preference (null = follow system) and return the effective mode. */
export async function applyThemePreference(
  preference: string | null
): Promise<ThemeMode> {
  currentPreference = preference;
  const tokens = await resolveTokens(preference);
  return applyThemeTokens(tokens);
}

export function getThemePreference(): string | null {
  return currentPreference;
}

/** The tokens currently applied to the shell (null before the first apply). */
export function currentThemeTokens(): ThemeTokens | null {
  return currentTokens;
}

/** Subscribe to token changes; returns an unsubscribe function. */
export function onThemeTokens(listener: (tokens: ThemeTokens) => void): () => void {
  tokenListeners.add(listener);
  return () => tokenListeners.delete(listener);
}

/** Subscribe to effective-mode changes (multi-slot; P3-6: the old single-slot
 *  field would silently overwrite a previous listener). Returns unsubscribe. */
export function onThemeModeChange(listener: (mode: ThemeMode) => void): () => void {
  modeListeners.add(listener);
  return () => modeListeners.delete(listener);
}

/** Initialize on app start; follows system changes when not pinned. */
export async function initTheme(): Promise<void> {
  const res = await dashboard.getThemePreference();
  const preference = res.ok ? res.data : null;
  await applyThemePreference(preference);

  systemMedia = window.matchMedia?.("(prefers-color-scheme: dark)") ?? null;
  systemMedia?.addEventListener("change", () => {
    if (currentPreference === null || currentPreference === "system") {
      void applyThemePreference(currentPreference);
    }
  });
}

/** Persist + apply a selection. `""` = follow system. */
export async function setThemePreference(
  preference: string | null
): Promise<{ ok: boolean; error?: string }> {
  const res = await dashboard.setTheme(preference ?? "");
  if (!res.ok) {
    return { ok: false, error: res.error.message };
  }
  await applyThemePreference(preference);
  return { ok: true };
}

/** Theme options for the settings page. */
export async function listThemes(): Promise<ThemeInfo[]> {
  const res = await dashboard.listThemes();
  return res.ok ? res.data : [];
}
