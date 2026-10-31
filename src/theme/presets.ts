/**
 * Host-owned visual presets for Noctis (theme v3).
 *
 * The public theme schema is still the 8-color `ThemeTokens` contract. A theme
 * preset only adds host-owned decoration around those colors: a secondary and
 * tertiary hue, the atmosphere family and the particle motif. Theme plugins
 * never provide CSS or arbitrary assets through this layer.
 */
import type { ThemeTokens } from "../ipc/types";

export type ThemeMood = "dark" | "light";

export type Atmosphere =
  | "noctis"
  | "verdant"
  | "abyss"
  | "ember"
  | "rose"
  | "parchment"
  | "daybreak"
  | "custom";

export type ParticleShape =
  | "mote"
  | "leaf"
  | "bubble"
  | "spark"
  | "petal"
  | "dust"
  | "shard";

export interface ThemeScene {
  key: string;
  label: string;
  atmosphere: Atmosphere;
  primary: string;
  secondary: string;
  tertiary: string;
  particle: ParticleShape;
  /** Base particle count at 1440px; the component scales it by viewport width. */
  particleCount: number;
  particleSpeed: number;
  particleOpacity: number;
}

/** Host-owned decorative palette. The public theme schema remains unchanged. */
export interface VisualPalette {
  primary: string;
  secondary: string;
  tertiary: string;
}

const SCENES: Record<string, ThemeScene> = {
  "builtin:dark": {
    key: "builtin:dark",
    label: "Noctis",
    atmosphere: "noctis",
    primary: "#efb35e",
    secondary: "#d96b4f",
    tertiary: "#73c9d4",
    particle: "mote",
    particleCount: 34,
    particleSpeed: 26,
    particleOpacity: 0.65,
  },
  "builtin:forest": {
    key: "builtin:forest",
    label: "Verdant",
    atmosphere: "verdant",
    primary: "#a3d46a",
    secondary: "#5d9e73",
    tertiary: "#e0c27a",
    particle: "leaf",
    particleCount: 26,
    particleSpeed: 30,
    particleOpacity: 0.55,
  },
  "builtin:ocean": {
    key: "builtin:ocean",
    label: "Abyss",
    atmosphere: "abyss",
    primary: "#5ad1c8",
    secondary: "#3a8fbf",
    tertiary: "#a7b8ff",
    particle: "bubble",
    particleCount: 28,
    particleSpeed: 24,
    particleOpacity: 0.55,
  },
  "builtin:ember": {
    key: "builtin:ember",
    label: "Ember",
    atmosphere: "ember",
    primary: "#ff8152",
    secondary: "#e0b15c",
    tertiary: "#d94f4f",
    particle: "spark",
    particleCount: 32,
    particleSpeed: 18,
    particleOpacity: 0.68,
  },
  "builtin:light": {
    key: "builtin:light",
    label: "Dawn",
    atmosphere: "daybreak",
    primary: "#b0442b",
    secondary: "#c8893f",
    tertiary: "#3f7772",
    particle: "dust",
    particleCount: 22,
    particleSpeed: 32,
    particleOpacity: 0.4,
  },
  "builtin:sakura": {
    key: "builtin:sakura",
    label: "Sakura",
    atmosphere: "rose",
    primary: "#c44963",
    secondary: "#a06cc9",
    tertiary: "#e3a85f",
    particle: "petal",
    particleCount: 24,
    particleSpeed: 28,
    particleOpacity: 0.5,
  },
  "builtin:paper": {
    key: "builtin:paper",
    label: "Parchment",
    atmosphere: "parchment",
    primary: "#8f5f1f",
    secondary: "#5b7b5c",
    tertiary: "#a64b3e",
    particle: "dust",
    particleCount: 20,
    particleSpeed: 34,
    particleOpacity: 0.35,
  },
  "builtin:sky": {
    key: "builtin:sky",
    label: "Daybreak",
    atmosphere: "daybreak",
    primary: "#2b66c9",
    secondary: "#4fa6b8",
    tertiary: "#6d79d8",
    particle: "shard",
    particleCount: 22,
    particleSpeed: 30,
    particleOpacity: 0.42,
  },
};

/** Re-exported for the theme preview cards and tests. */
export const THEME_SCENES = SCENES;

export function isDarkHex(hex: string): boolean {
  const rgb = parseHex(hex);
  if (!rgb) return true;
  return (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]) / 255 < 0.5;
}

function parseHex(hex: string): [number, number, number] | null {
  const value = hex.trim().replace(/^#/, "");
  if (!/^[0-9a-f]{3}$/i.test(value) && !/^[0-9a-f]{6}$/i.test(value)) return null;
  const full =
    value.length === 3
      ? value
          .split("")
          .map((c) => c + c)
          .join("")
      : value;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

function hex(rgb: [number, number, number]): string {
  return `#${rgb
    .map((channel) => Math.max(0, Math.min(255, Math.round(channel))).toString(16).padStart(2, "0"))
    .join("")}`;
}

function rgbToHsl(rgb: [number, number, number]): [number, number, number] {
  const r = rgb[0] / 255;
  const g = rgb[1] / 255;
  const b = rgb[2] / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / d + 2) / 6;
  else h = ((r - g) / d + 4) / 6;
  return [h * 360, s, l];
}

function hslToHex(h: number, s: number, l: number): string {
  const hue = ((h % 360) + 360) % 360;
  const sat = Math.max(0, Math.min(1, s));
  const light = Math.max(0, Math.min(1, l));
  const c = (1 - Math.abs(2 * light - 1)) * sat;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = light - c / 2;
  let rgb: [number, number, number];
  if (hue < 60) rgb = [c, x, 0];
  else if (hue < 120) rgb = [x, c, 0];
  else if (hue < 180) rgb = [0, c, x];
  else if (hue < 240) rgb = [0, x, c];
  else if (hue < 300) rgb = [x, 0, c];
  else rgb = [c, 0, x];
  return hex([(rgb[0] + m) * 255, (rgb[1] + m) * 255, (rgb[2] + m) * 255]);
}

function shifted(base: string, degrees: number, mode: ThemeMood, saturationScale = 1): string {
  const rgb = parseHex(base);
  if (!rgb) return mode === "dark" ? "#8b93a8" : "#6b6257";
  const [h, s, l] = rgbToHsl(rgb);
  const targetL = mode === "dark" ? Math.max(0.5, Math.min(0.74, l)) : Math.max(0.34, Math.min(0.56, l));
  return hslToHex(h + degrees, Math.max(0.18, Math.min(0.72, s * saturationScale)), targetL);
}

function customScene(preference: string, tokens: ThemeTokens, mode: ThemeMood): ThemeScene {
  const primary = tokens.accent;
  return {
    key: preference || "custom",
    label: "Custom",
    atmosphere: "custom",
    primary,
    secondary: shifted(primary, mode === "dark" ? 38 : -34, mode),
    tertiary: shifted(primary, mode === "dark" ? -58 : 56, mode, 0.82),
    particle: "mote",
    particleCount: 24,
    particleSpeed: 30,
    particleOpacity: 0.5,
  };
}

/** Resolve the active host preset. Unknown/plugin themes get a deterministic
 *  derived scene so every theme, including v1 plugin themes, gets a backdrop. */
export function sceneFor(
  preference: string | null,
  tokens: ThemeTokens,
  mode: ThemeMood = isDarkHex(tokens.background) ? "dark" : "light"
): ThemeScene {
  const key = !preference || preference === "system" ? (mode === "dark" ? "builtin:dark" : "builtin:light") : preference;
  const known = SCENES[key];
  if (!known) return customScene(key, tokens, mode);
  // The token accent is the public contract; keep it authoritative.
  return { ...known, primary: tokens.accent, key };
}

export function visualPaletteFor(
  preference: string | null,
  tokens: ThemeTokens,
  mode: ThemeMood = isDarkHex(tokens.background) ? "dark" : "light"
): VisualPalette {
  const scene = sceneFor(preference, tokens, mode);
  return { primary: scene.primary, secondary: scene.secondary, tertiary: scene.tertiary };
}
