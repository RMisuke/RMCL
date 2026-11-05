/**
 * Theme V4 palette derivation (docs/ui-design/15_THEME_SYSTEM_V4.md §3).
 *
 * A theme only stores three base colors; every other token is derived so the
 * palette stays internally consistent and the settings page only exposes the
 * three colors the user actually cares about.
 */
import type { ThemeTokens } from "../../ipc/types";

export interface Base3 {
  accent: string;
  background: string;
  text: string;
}

type Rgb = [number, number, number];

export function parseHex(hex: string): Rgb | null {
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

function toHex(rgb: Rgb): string {
  return `#${rgb
    .map((channel) => Math.max(0, Math.min(255, Math.round(channel))).toString(16).padStart(2, "0"))
    .join("")}`;
}

/** Linear RGB mix: `amount` of `top` over `base`. */
export function mix(base: string, top: string, amount: number): string {
  const a = parseHex(base);
  const b = parseHex(top);
  if (!a || !b) return base;
  const t = Math.max(0, Math.min(1, amount));
  return toHex([
    a[0] + (b[0] - a[0]) * t,
    a[1] + (b[1] - a[1]) * t,
    a[2] + (b[2] - a[2]) * t,
  ]);
}

/** Relative luminance (0..1) used to pick a readable accent-contrast color. */
export function luminance(hex: string): number {
  const rgb = parseHex(hex);
  if (!rgb) return 0;
  return (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]) / 255;
}

export function isDarkBase(base: Base3): boolean {
  return luminance(base.background) < 0.5;
}

/** Derive the full public token set from three base colors. */
export function deriveTokens(base: Base3): ThemeTokens {
  const { accent, background, text } = base;
  return {
    background,
    surface: mix(background, text, 0.05),
    surfaceElevated: mix(background, text, 0.09),
    text,
    textMuted: mix(text, background, 0.4),
    accent,
    accentContrast: luminance(accent) > 0.62 ? "#15100a" : "#fff8f2",
    border: mix(background, text, 0.16),
  };
}
