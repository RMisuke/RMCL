/**
 * Pure helpers shared by the instance settings form.
 *
 * The form deliberately keeps every overridable value in the *effective* shape
 * (global preset merged with the instance overlay). Editing any control in a
 * settings group first freezes that whole group into `override_properties`, so
 * what the user sees in the form is always what the launcher will use.
 */
import type {
  JavaInfo,
  ResolvedInstanceSettings,
} from "../ipc/types";

export const JAVA_FIELDS = [
  "java_type",
  "custom_java_path",
  "custom_java_version",
  "detected_java_version",
  "detected_java_path_hash",
] as const;

export const MEMORY_FIELDS = ["max_memory_mb", "min_memory_mb", "metaspace_mb"] as const;
export const WINDOW_FIELDS = ["window_type", "width", "height"] as const;
export const QUICK_PLAY_FIELDS = [
  "quick_play",
  "quick_play_multiplayer",
  "quick_play_singleplayer",
  "quick_play_realms",
] as const;
export const PROCESS_FIELDS = ["process_priority", "running_directory"] as const;
export const COMMAND_FIELDS = [
  "wrapper",
  "pre_launch_command",
  "post_exit_command",
] as const;
export const JVM_FIELDS = [
  "jvm_args",
  "no_generated_jvm_args",
  "no_generated_optimizing_jvm_args",
  "not_check_jvm",
] as const;
export const GAME_FIELDS = ["game_args", "env_vars"] as const;
export const AGENT_FIELDS = ["java_agents"] as const;
export const NATIVE_FIELDS = [
  "use_native_glfw",
  "use_native_openal",
  "use_custom_natives",
] as const;
export const DEBUG_FIELDS = ["enable_debug_log_output"] as const;

/** A card-level inheritance group (one lock freezes the whole card). */
export type SettingGroupId =
  | "java"
  | "memory"
  | "window"
  | "quickPlay"
  | "process"
  | "commands"
  | "jvm"
  | "game"
  | "agents"
  | "natives"
  | "debug";

export const GROUP_FIELDS: Record<SettingGroupId, readonly string[]> = {
  java: JAVA_FIELDS,
  memory: MEMORY_FIELDS,
  window: WINDOW_FIELDS,
  quickPlay: QUICK_PLAY_FIELDS,
  process: PROCESS_FIELDS,
  commands: COMMAND_FIELDS,
  jvm: JVM_FIELDS,
  game: GAME_FIELDS,
  agents: AGENT_FIELDS,
  natives: NATIVE_FIELDS,
  debug: DEBUG_FIELDS,
};

/** Which card group owns `field`; `null` for non-inheritable fields. */
export function groupForField(field: string): readonly string[] | null {
  for (const fields of Object.values(GROUP_FIELDS)) {
    if (fields.includes(field)) return fields;
  }
  return null;
}

/** Add every field of a group to the override list, preserving unrelated ones. */
export function addGroupOverrides(
  overrides: readonly string[],
  fields: readonly string[]
): string[] {
  const next = new Set(overrides);
  for (const field of fields) next.add(field);
  return [...next];
}

/** Remove every field of a group from the override list. */
export function removeGroupOverrides(
  overrides: readonly string[],
  fields: readonly string[]
): string[] {
  const remove = new Set(fields);
  return overrides.filter((field) => !remove.has(field));
}

export function isGroupOverridden(
  overrides: readonly string[],
  fields: readonly string[]
): boolean {
  return fields.some((field) => overrides.includes(field));
}

/**
 * Serialize the given fields from the effective-shaped draft into a backend
 * patch. Nullable integers become `0` (the backend's clear convention) and a
 * nullable path becomes `""` (also clear).
 */
export function buildFieldPatch(
  draft: ResolvedInstanceSettings,
  fields: readonly string[]
): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  for (const field of fields) {
    switch (field) {
      case "min_memory_mb":
      case "metaspace_mb":
        patch[field] = draft[field] ?? 0;
        break;
      case "running_directory":
        patch[field] = draft.running_directory ?? "";
        break;
      case "env_vars":
        patch[field] = { ...draft.env_vars };
        break;
      case "jvm_args":
      case "game_args":
      case "java_agents":
        patch[field] = [...draft[field]];
        break;
      default:
        patch[field] = (draft as unknown as Record<string, unknown>)[field];
        break;
    }
  }
  return patch;
}

/** Copy only the requested fields from a fresh effective snapshot. */
export function pickEffective(
  current: ResolvedInstanceSettings,
  effective: ResolvedInstanceSettings,
  fields: readonly string[]
): ResolvedInstanceSettings {
  const next = { ...current } as unknown as Record<string, unknown>;
  for (const field of fields) {
    next[field] = (effective as unknown as Record<string, unknown>)[field];
  }
  return next as unknown as ResolvedInstanceSettings;
}

// ── Memory ──

export interface MemoryLimits {
  totalMb: number;
  /** Highest value the slider/input offers. */
  maxMb: number;
  suggestedMb: number;
}

/**
 * Derive usable bounds from physical memory.
 *
 * The launcher never invents a host size: when the probe is unavailable the
 * caller falls back to `totalMb = 0`, and these limits stay at the historical
 * 512..32768 MiB window.
 */
export function memoryLimits(totalBytes: number): MemoryLimits {
  const totalMb = Math.max(0, Math.floor(totalBytes / (1024 * 1024)));
  if (totalMb === 0) {
    return { totalMb, maxMb: 32768, suggestedMb: 4096 };
  }
  const maxMb = Math.min(32768, Math.max(4096, Math.floor(totalMb / 512) * 512));
  const suggestion = Math.floor((totalMb * 0.5) / 512) * 512;
  return {
    totalMb,
    maxMb,
    suggestedMb: Math.min(maxMb, Math.max(2048, Math.min(8192, suggestion))),
  };
}

/** Clamp an arbitrary memory input to the 512 MiB grid used by the UI. */
export function clampMemoryMb(value: number, maxMb: number): number {
  if (!Number.isFinite(value)) return 512;
  const rounded = Math.round(value / 512) * 512;
  return Math.max(512, Math.min(maxMb, rounded));
}

/** `4096` → `4 GiB`; below 1 GiB stay in MiB. */
export function formatMemoryMb(value: number | null | undefined): string {
  const mb = Math.max(0, Math.round(value ?? 0));
  if (mb >= 1024) {
    const gib = mb / 1024;
    return `${Number.isInteger(gib) ? gib : gib.toFixed(1)} GiB`;
  }
  return `${mb} MiB`;
}

// ── Java ──

/**
 * Minimum Java major version for a Minecraft version, mirroring the launcher's
 * own rule table closely enough for an inline advisory. Only a hint: the
 * backend still performs the authoritative check before launch.
 */
export function requiredJavaFor(gameVersion: string): number {
  const match = /^(\d+)(?:\.(\d+))?(?:\.(\d+))?/.exec(gameVersion.trim());
  if (!match) return 21;
  const major = Number(match[1]);
  const minor = Number(match[2] ?? 0);
  const patch = Number(match[3] ?? 0);
  if (major > 1) return 21;
  if (minor > 20 || (minor === 20 && patch >= 5)) return 21;
  if (minor >= 18) return 17;
  if (minor === 17) return 16;
  return 8;
}

export function javaRuntimeLabel(java: JavaInfo): string {
  const vendor = java.vendor.trim() || "Unknown";
  const bits = java.is_64bit ? "64-bit" : "32-bit";
  return `Java ${java.major_version} · ${vendor} · ${bits}`;
}

/** Find the detected runtime a custom path points at, if any. */
export function detectedJavaFor(
  path: string,
  javas: readonly JavaInfo[]
): JavaInfo | undefined {
  const needle = path.trim();
  if (!needle) return undefined;
  return javas.find((java) => java.path === needle);
}

// ── Window ──

export interface ResolutionPreset {
  width: number;
  height: number;
  label: string;
}

export const RESOLUTION_PRESETS: ResolutionPreset[] = [
  { width: 854, height: 480, label: "854 × 480" },
  { width: 1280, height: 720, label: "1280 × 720" },
  { width: 1366, height: 768, label: "1366 × 768" },
  { width: 1600, height: 900, label: "1600 × 900" },
  { width: 1920, height: 1080, label: "1920 × 1080" },
  { width: 2560, height: 1440, label: "2560 × 1440" },
  { width: 3840, height: 2160, label: "3840 × 2160" },
];

export function resolutionKey(width: number, height: number): string {
  return `${width}x${height}`;
}

export function matchingResolution(width: number, height: number): ResolutionPreset | null {
  return RESOLUTION_PRESETS.find((p) => p.width === width && p.height === height) ?? null;
}

// ── Environment variables ──

export function formatEnvText(env: Record<string, string | undefined>): string {
  return Object.entries(env)
    .filter((entry): entry is [string, string] => typeof entry[1] === "string")
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

/**
 * Parse `KEY=VALUE` lines. Invalid lines are ignored rather than stored as a
 * broken environment; the UI shows a hint next to the editor.
 */
export function parseEnvText(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const split = trimmed.indexOf("=");
    if (split <= 0) continue;
    const key = trimmed.slice(0, split).trim();
    if (!key) continue;
    env[key] = trimmed.slice(split + 1).trim();
  }
  return env;
}
