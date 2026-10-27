/**
 * Legacy-shaped view over a version-backed `InstanceDetail`.
 *
 * The settings page grew up around the old `Profile` record. Until that page is
 * rewritten field-by-field (next frontend slice), this adapter keeps the JSX
 * stable while every value comes from the new instance backend.
 */
import type {
  InstanceDetail,
  InstanceIcon,
  InstanceSettingsPatch,
  ProcessPriority,
  WindowType,
  JavaType,
} from "../ipc/types";

export interface InstanceLoaderView {
  loader_type: string;
  loader_version: string;
}

export interface InstanceView {
  /** Composite `{directory}/{instance}` key — the only id commands accept. */
  id: string;
  instance_id: string;
  directory_id: string;
  name: string;
  game_version: string;
  mod_loader: InstanceLoaderView | null;
  game_dir: string;
  instance_dir: string;
  run_dir: string;
  external_root: boolean;
  icon: string | null;
  created_at: string | null;
  last_played: string | null;
  play_time_seconds: number;
  isolation: string | null;
  override_properties: string[];
  java_path: string | null;
  jvm_args: string[];
  game_args: string[];
  java_agents: string[];
  memory_mb: number;
  min_memory_mb: number;
  metaspace_mb: number;
  process_priority: string;
  width: number | null;
  height: number | null;
  fullscreen: boolean;
  use_native_glfw: boolean;
  use_native_openal: boolean;
  use_custom_natives: boolean;
  no_generated_jvm_args: boolean;
  no_generated_optimizing_jvm_args: boolean;
  enable_debug_log_output: boolean;
  wrapper: string | null;
  override_jvm_args: string[];
  pre_launch_command: string | null;
  post_exit_command: string | null;
  env_vars: Record<string, string>;
  settings_read_only: boolean;
}

export function iconUrl(icon: InstanceIcon): string | null {
  return icon.type === "data_url" || icon.type === "file" ? icon.value : null;
}

function priorityLabel(value: string): string {
  return value;
}

function legacyOverrideNames(names: string[]): string[] {
  const out = new Set<string>();
  for (const name of names) {
    switch (name) {
      case "java_type":
      case "custom_java_path":
        out.add("java_path");
        break;
      case "max_memory_mb":
        out.add("memory_mb");
        break;
      case "window_type":
        out.add("fullscreen");
        break;
      default:
        out.add(name);
    }
  }
  return [...out];
}

function newOverrideNames(names: string[]): string[] {
  const out = new Set<string>();
  for (const name of names) {
    switch (name) {
      case "java_path":
        out.add("java_type");
        out.add("custom_java_path");
        break;
      case "memory_mb":
        out.add("max_memory_mb");
        break;
      case "fullscreen":
        out.add("window_type");
        break;
      case "override_jvm_args":
        out.add("jvm_args");
        break;
      default:
        out.add(name);
    }
  }
  return [...out];
}

export function toInstanceView(detail: InstanceDetail): InstanceView {
  const s = detail.settings;
  const e = detail.effective;
  return {
    id: detail.summary.key,
    instance_id: detail.summary.id,
    directory_id: detail.summary.directory_id,
    name: detail.summary.display_name,
    game_version: detail.summary.game_version,
    mod_loader: detail.summary.loader
      ? {
          loader_type: detail.summary.loader.kind,
          loader_version: detail.summary.loader.version ?? "",
        }
      : null,
    game_dir: detail.paths.game_dir,
    instance_dir: detail.paths.instance_dir,
    run_dir: detail.paths.run_dir,
    external_root: detail.directory.kind === "external",
    icon: iconUrl(detail.summary.icon),
    created_at: detail.summary.created_at,
    last_played: detail.summary.last_played,
    play_time_seconds: Number(detail.summary.play_time_seconds),
    isolation: s.isolation ?? e.isolation,
    override_properties: legacyOverrideNames(s.override_properties),
    java_path:
      s.java_type === "custom" && s.custom_java_path ? s.custom_java_path : null,
    jvm_args: [...s.jvm_args],
    game_args: [...s.game_args],
    java_agents: [...s.java_agents],
    memory_mb: s.max_memory_mb,
    min_memory_mb: s.min_memory_mb ?? 0,
    metaspace_mb: s.metaspace_mb ?? 0,
    process_priority: priorityLabel(s.process_priority),
    width: s.width,
    height: s.height,
    fullscreen: s.window_type === "fullscreen",
    use_native_glfw: s.use_native_glfw,
    use_native_openal: s.use_native_openal,
    use_custom_natives: s.use_custom_natives,
    no_generated_jvm_args: s.no_generated_jvm_args,
    no_generated_optimizing_jvm_args: s.no_generated_optimizing_jvm_args,
    enable_debug_log_output: s.enable_debug_log_output,
    wrapper: s.wrapper || null,
    override_jvm_args: [],
    pre_launch_command: s.pre_launch_command || null,
    post_exit_command: s.post_exit_command || null,
    env_vars: Object.fromEntries(
      Object.entries(s.env_vars).filter((entry): entry is [string, string] => typeof entry[1] === "string")
    ),
    settings_read_only: detail.summary.settings_read_only,
  };
}

/** Convert the old settings-form patch into the new backend patch. */
export function toSettingsPatch(opts: Record<string, unknown>): InstanceSettingsPatch {
  const patch = {} as InstanceSettingsPatch;
  if (typeof opts.name === "string") patch.display_name = opts.name;
  if (opts.icon === null || typeof opts.icon === "string") {
    const value = opts.icon as string | null;
    patch.icon = !value
      ? { type: "auto" }
      : value.startsWith("data:")
        ? { type: "data_url", value }
        : { type: "file", value };
  }
  if (typeof opts.isolation === "string") patch.isolation = opts.isolation;
  if (Array.isArray(opts.override_properties)) {
    patch.override_properties = newOverrideNames(
      (opts.override_properties as string[]).filter((name) => typeof name === "string")
    );
  }
  if (typeof opts.java_path === "string") {
    patch.java_type = opts.java_path ? ("custom" as JavaType) : ("auto" as JavaType);
    patch.custom_java_path = opts.java_path;
  }
  if (Array.isArray(opts.jvm_args)) patch.jvm_args = opts.jvm_args as string[];
  if (Array.isArray(opts.game_args)) patch.game_args = opts.game_args as string[];
  if (Array.isArray(opts.java_agents)) patch.java_agents = opts.java_agents as string[];
  if (typeof opts.memory_mb === "number") patch.max_memory_mb = opts.memory_mb;
  if (typeof opts.min_memory_mb === "number") patch.min_memory_mb = opts.min_memory_mb;
  if (typeof opts.metaspace_mb === "number") patch.metaspace_mb = opts.metaspace_mb;
  if (typeof opts.process_priority === "string") {
    patch.process_priority = opts.process_priority as ProcessPriority;
  }
  if (typeof opts.width === "number") patch.width = opts.width;
  if (typeof opts.height === "number") patch.height = opts.height;
  if (typeof opts.fullscreen === "boolean") {
    patch.window_type = (opts.fullscreen ? "fullscreen" : "windowed") as WindowType;
  }
  for (const key of [
    "use_native_glfw",
    "use_native_openal",
    "use_custom_natives",
    "no_generated_jvm_args",
    "no_generated_optimizing_jvm_args",
    "enable_debug_log_output",
  ] as const) {
    if (typeof opts[key] === "boolean") {
      (patch as Record<string, unknown>)[key] = opts[key];
    }
  }
  if (opts.wrapper === null || typeof opts.wrapper === "string") {
    patch.wrapper = (opts.wrapper as string | null) ?? "";
  }
  if (opts.pre_launch_command === null || typeof opts.pre_launch_command === "string") {
    patch.pre_launch_command = (opts.pre_launch_command as string | null) ?? "";
  }
  if (opts.post_exit_command === null || typeof opts.post_exit_command === "string") {
    patch.post_exit_command = (opts.post_exit_command as string | null) ?? "";
  }
  if (opts.env_vars && typeof opts.env_vars === "object") {
    patch.env_vars = opts.env_vars as Record<string, string>;
  }
  return patch;
}
