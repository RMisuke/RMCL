/**
 * Theme token application (Phase 3, ADR-005).
 *
 * The backend owns token validation; this module maps tokens to CSS variables
 * and bridges them to the legacy `--surface-*` / `--text-*` variables so the
 * existing stylesheet keeps working while new widgets use `--rmccc-*`.
 */

import { createSignal } from "solid-js";
import { dashboard } from "../ipc/api";
import type { ThemeInfo, ThemeSkin, ThemeSkinPayload, ThemeTokens } from "../ipc/types";
import { isDarkHex, sceneFor, type ThemeScene } from "./presets";

export { visualPaletteFor } from "./presets";
export type { ThemeMood, ThemeScene } from "./presets";

const DARK_FALLBACK: ThemeTokens = {
  background: "#0b0c0f",
  surface: "#13151a",
  surfaceElevated: "#1a1d24",
  text: "#f2eee7",
  textMuted: "#a39d92",
  accent: "#efb35e",
  accentContrast: "#1b1204",
  border: "#2b2e36",
};

const LIGHT_FALLBACK: ThemeTokens = {
  background: "#f1ece2",
  surface: "#fbf7ef",
  surfaceElevated: "#fffdf7",
  text: "#1a1713",
  textMuted: "#786f63",
  accent: "#b0442b",
  accentContrast: "#fff9f2",
  border: "#ded5c6",
};

/** Effective mode for host-owned safety colors. */
export type ThemeMode = "light" | "dark";

/** Host-owned scene for the active theme (atmosphere + particle motif). */
const [currentScene, setCurrentScene] = createSignal<ThemeScene | null>(null);
export function themeScene(): ThemeScene | null {
  return currentScene();
}

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
  return isDarkHex(hex);
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

  const scene = sceneFor(currentPreference, tokens, mode);
  setCurrentScene(scene);
  setVar("--rmccc-accent-secondary", scene.secondary);
  setVar("--rmccc-accent-tertiary", scene.tertiary);
  setVar("--rmccc-accent-gradient", `linear-gradient(135deg, ${scene.primary}, ${scene.secondary})`);
  setVar("--scene-primary", scene.primary);
  setVar("--scene-secondary", scene.secondary);
  setVar("--scene-tertiary", scene.tertiary);
  setVar("--scene-particle-opacity", String(scene.particleOpacity));
  // Neutral atmosphere values: decor is allowed to be colourful, content is not.
  setVar("--scene-wash", `color-mix(in srgb, ${scene.primary} 12%, transparent)`);
  setVar("--scene-wash-2", `color-mix(in srgb, ${scene.secondary} 10%, transparent)`);
  setVar("--scene-line", `color-mix(in srgb, ${scene.primary} 22%, transparent)`);
  root.dataset.visualTheme = currentPreference?.replace("builtin:", "") ?? mode;
  root.dataset.themeKey = currentPreference ?? (mode === "dark" ? "builtin:dark" : "builtin:light");
  root.dataset.atmosphere = scene.atmosphere;

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
  // Legacy `light` / `dark` values are normalized so the scene lookup,
  // settings selection and theme skin all share one stable key.
  const normalized =
    preference === "light"
      ? "builtin:light"
      : preference === "dark"
        ? "builtin:dark"
        : preference;
  currentPreference = normalized;
  const tokens = await resolveTokens(normalized);
  const mode = applyThemeTokens(tokens);
  await refreshThemeSkin(normalized);
  return mode;
}

// ── Theme v2 skin (ADR-014) ───────────────────────────────────────────────
//
// The backend resolves theme assets into `rmcccplugin://` URLs and validates
// every value. This module only maps the already-trusted skin to CSS variables
// and host-owned markup (effects/backdrop/nav icons); themes never provide CSS.

const [currentSkin, setCurrentSkin] = createSignal<ThemeSkin | null>(null);
const [skinMeta, setSkinMeta] = createSignal({
  requiresSkin: false,
  granted: false,
});

/** Active v2 skin, or null for built-in / color-only themes. */
export function themeSkin(): ThemeSkin | null {
  return currentSkin();
}

/** Whether the active theme wants skin features and whether the user granted them. */
export function themeSkinMeta(): { requiresSkin: boolean; granted: boolean } {
  return skinMeta();
}

const SKIN_SLOTS = [
  "card",
  "button",
  "input",
  "menu",
  "dialog",
  "tag",
  "hero",
  "titlebar",
];
const appliedSkinVars = new Set<string>();
let fontStyleElement: HTMLStyleElement | null = null;

function setSkinVar(name: string, value: string) {
  document.documentElement.style.setProperty(name, value);
  appliedSkinVars.add(name);
}

function clearSkin() {
  const root = document.documentElement;
  for (const name of appliedSkinVars) root.style.removeProperty(name);
  appliedSkinVars.clear();
  fontStyleElement?.remove();
  fontStyleElement = null;
  delete root.dataset.skin;
  delete root.dataset.skinPill;
  delete root.dataset.skinBackground;
  for (const slot of SKIN_SLOTS) {
    delete root.dataset[`skin${slot[0].toUpperCase()}${slot.slice(1)}Image`];
  }
  setCurrentSkin(null);
  setSkinMeta({ requiresSkin: false, granted: false });
}

function fontStackFor(face: { family: string; fallback: string }): string {
  return `"${face.family}", ${face.fallback}`;
}

function fontFacesCss(skin: ThemeSkin): string {
  const faces = [
    skin.typography?.ui,
    skin.typography?.display,
    skin.typography?.mono,
  ].filter((face): face is NonNullable<typeof face> => Boolean(face));
  let css = "";
  for (const face of faces) {
    for (const source of face.sources) {
      const format = source.url.endsWith(".woff2")
        ? "woff2"
        : source.url.endsWith(".woff")
          ? "woff"
          : source.url.endsWith(".otf")
            ? "opentype"
            : "truetype";
      css += `@font-face{font-family:"${face.family}";src:url("${source.url}") format("${format}");font-weight:${source.weight};font-style:${source.style};font-display:${source.display};}`;
    }
  }
  return css;
}

/** Apply a backend-resolved skin payload (or clear it for color-only themes). */
export function applyThemeSkinPayload(payload: ThemeSkinPayload) {
  clearSkin();
  setSkinMeta({
    requiresSkin: payload.requiresSkin,
    granted: payload.granted,
  });
  const skin = payload.skin;
  if (!skin) return;
  setCurrentSkin(skin);

  const root = document.documentElement;
  root.dataset.skin = "on";

  const shape = skin.shape;
  if (shape) {
    if (shape.tag != null) setSkinVar("--rmccc-radius-tag", `${shape.tag}px`);
    if (shape.control != null)
      setSkinVar("--rmccc-radius-control", `${shape.control}px`);
    if (shape.card != null) setSkinVar("--rmccc-radius-card", `${shape.card}px`);
    if (shape.modal != null)
      setSkinVar("--rmccc-radius-modal", `${shape.modal}px`);
    if (shape.pill) root.dataset.skinPill = "on";
  }

  const background = skin.background;
  if (background) root.dataset.skinBackground = background.scope;

  const unit = skin.spacing?.unit;
  if (unit) {
    [unit, unit * 2, unit * 3, unit * 4, unit * 6, unit * 8].forEach(
      (value, index) => setSkinVar(`--rmccc-space-${index + 1}`, `${value}px`)
    );
  }

  if (skin.typography?.ui)
    setSkinVar("--font-stack", fontStackFor(skin.typography.ui));
  if (skin.typography?.display)
    setSkinVar("--font-display", fontStackFor(skin.typography.display));
  if (skin.typography?.mono)
    setSkinVar("--font-mono", fontStackFor(skin.typography.mono));

  const faceCss = fontFacesCss(skin);
  if (faceCss) {
    const style = document.createElement("style");
    style.id = "rmccc-theme-fonts";
    style.textContent = faceCss;
    document.head.appendChild(style);
    fontStyleElement = style;
  }

  for (const [slot, component] of Object.entries(skin.components ?? {})) {
    if (!component) continue;
    const prefix = `--rmccc-skin-${slot}-`;
    if (component.background)
      setSkinVar(`${prefix}bg`, component.background);
    if (component.border) setSkinVar(`${prefix}border`, component.border);
    if (component.text) setSkinVar(`${prefix}text`, component.text);
    if (component.radius != null)
      setSkinVar(`${prefix}radius`, `${component.radius}px`);
    if (component.blur != null)
      setSkinVar(`${prefix}blur`, `${component.blur}px`);
    if (component.padding != null)
      setSkinVar(`${prefix}padding`, `${component.padding}px`);
    if (component.shadow)
      setSkinVar(`${prefix}shadow`, `var(--rmccc-shadow-${component.shadow})`);
    if (component.primaryBackground)
      setSkinVar(`${prefix}primary-bg`, component.primaryBackground);
    if (component.primaryText)
      setSkinVar(`${prefix}primary-text`, component.primaryText);
    if (component.image) {
      setSkinVar(`${prefix}image`, `url("${component.image}")`);
      if (component.imageSlice != null)
        setSkinVar(`${prefix}slice`, `${component.imageSlice}`);
      root.setAttribute(
        `data-skin-${slot}-image`,
        component.imageMode ?? "background"
      );
    }
  }
}

async function refreshThemeSkin(preference: string | null) {
  if (
    !preference ||
    preference === "system" ||
    preference === "light" ||
    preference === "dark"
  ) {
    clearSkin();
    return;
  }
  const res = await dashboard.getThemeSkin(preference);
  if (!res.ok) {
    console.warn("theme: could not load skin", res.error);
    clearSkin();
    return;
  }
  applyThemeSkinPayload(res.data);
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
