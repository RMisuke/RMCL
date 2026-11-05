/**
 * Theme V4 registry (docs/ui-design/15_THEME_SYSTEM_V4.md §4).
 *
 * Eight selectable palettes: six presets (each with a CSS motion backdrop and
 * day/night base colors), the wallpaper-adaptive palette and the user-defined
 * custom palette.
 */
import type { Base3 } from "./palette";

export type ThemeKind = "preset" | "adaptive" | "custom";

/** CSS motion backdrop identifier; `null` = no animated layer. */
export type MotionId =
  | "stellar"
  | "verdant"
  | "abyss"
  | "ember"
  | "sakura"
  | "dawn";

export interface ThemeDef {
  id: string;
  kind: ThemeKind;
  /** i18n key for the display name. */
  nameKey: string;
  motion: MotionId | null;
  /** Palette used when the day/night mode is off. */
  defaultMode: "day" | "night";
  day: Base3;
  night: Base3;
}

export const THEME_REGISTRY: ThemeDef[] = [
  {
    id: "stellar",
    kind: "preset",
    nameKey: "theme.stellar",
    motion: "stellar",
    defaultMode: "night",
    day: { accent: "#3f6fd8", background: "#eef3fb", text: "#1b2333" },
    night: { accent: "#8fb7ff", background: "#0b1020", text: "#e8ecf7" },
  },
  {
    id: "verdant",
    kind: "preset",
    nameKey: "theme.verdant",
    motion: "verdant",
    defaultMode: "night",
    day: { accent: "#3f8f4f", background: "#eef6ea", text: "#1c2a1c" },
    night: { accent: "#9fd478", background: "#0d1710", text: "#e6f0e2" },
  },
  {
    id: "abyss",
    kind: "preset",
    nameKey: "theme.abyss",
    motion: "abyss",
    defaultMode: "night",
    day: { accent: "#1f8f96", background: "#e8f5f6", text: "#123033" },
    night: { accent: "#5fd6cf", background: "#06131a", text: "#dceef2" },
  },
  {
    id: "ember",
    kind: "preset",
    nameKey: "theme.ember",
    motion: "ember",
    defaultMode: "night",
    day: { accent: "#d8552b", background: "#fbf0e8", text: "#33150c" },
    night: { accent: "#ff8a4d", background: "#170b08", text: "#f6e7dd" },
  },
  {
    id: "sakura",
    kind: "preset",
    nameKey: "theme.sakura",
    motion: "sakura",
    defaultMode: "night",
    day: { accent: "#d84f7d", background: "#fdf1f5", text: "#3a1622" },
    night: { accent: "#f27ba0", background: "#170b12", text: "#f6e8ee" },
  },
  {
    id: "dawn",
    kind: "preset",
    nameKey: "theme.dawn",
    motion: "dawn",
    defaultMode: "day",
    day: { accent: "#d8862b", background: "#fbf4e8", text: "#2c2415" },
    night: { accent: "#f2b04e", background: "#10131c", text: "#eef0f6" },
  },
  {
    id: "adaptive",
    kind: "adaptive",
    nameKey: "theme.adaptive",
    motion: null,
    defaultMode: "night",
    day: { accent: "#efb35e", background: "#13151a", text: "#f2eee7" },
    night: { accent: "#efb35e", background: "#13151a", text: "#f2eee7" },
  },
  {
    id: "custom",
    kind: "custom",
    nameKey: "theme.custom",
    motion: null,
    defaultMode: "night",
    day: { accent: "#efb35e", background: "#13151a", text: "#f2eee7" },
    night: { accent: "#efb35e", background: "#13151a", text: "#f2eee7" },
  },
];

export function themeById(id: string): ThemeDef | null {
  return THEME_REGISTRY.find((theme) => theme.id === id) ?? null;
}

/** Legacy backend preference → v4 theme id (doc §7). */
export function migrateLegacyPreference(preference: string | null): string {
  switch (preference) {
    case null:
    case "":
    case "system":
    case "builtin:dark":
    case "dark":
      return "stellar";
    case "builtin:light":
    case "light":
    case "builtin:paper":
      return "dawn";
    case "builtin:forest":
      return "verdant";
    case "builtin:ocean":
      return "abyss";
    case "builtin:ember":
      return "ember";
    case "builtin:sakura":
      return "sakura";
    case "builtin:sky":
      return "dawn";
    default:
      return "stellar";
  }
}
