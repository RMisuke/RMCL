import { describe, expect, it } from "vitest";
import {
  addGroupOverrides,
  buildFieldPatch,
  clampMemoryMb,
  formatMemoryMb,
  groupForField,
  isGroupOverridden,
  memoryLimits,
  parseEnvText,
  pickEffective,
  removeGroupOverrides,
  requiredJavaFor,
  RESOLUTION_PRESETS,
  matchingResolution,
} from "./instanceSettings";
import type { ResolvedInstanceSettings } from "../ipc/types";

function draft(overrides: Partial<ResolvedInstanceSettings> = {}): ResolvedInstanceSettings {
  return {
    isolation: "modded",
    java_type: "auto",
    custom_java_path: "",
    custom_java_version: "",
    detected_java_version: "",
    detected_java_path_hash: "",
    jvm_args: [],
    game_args: [],
    java_agents: [],
    no_generated_jvm_args: false,
    no_generated_optimizing_jvm_args: false,
    auto_memory: false,
    max_memory_mb: 4096,
    min_memory_mb: null,
    metaspace_mb: null,
    process_priority: "normal",
    window_type: "windowed",
    width: 854,
    height: 480,
    running_directory: null,
    launcher_visibility: "hide",
    allow_auto_agent: false,
    disable_auto_game_options: false,
    graphics_backend: "default",
    opengl_renderer: "",
    vulkan_renderer: "",
    env_vars: {},
    wrapper: "",
    pre_launch_command: "",
    post_exit_command: "",
    quick_play: "none",
    quick_play_multiplayer: "",
    quick_play_singleplayer: "",
    quick_play_realms: "",
    show_logs: false,
    enable_debug_log_output: false,
    not_check_jvm: false,
    not_check_game: false,
    not_patch_natives: false,
    use_custom_natives: false,
    natives_directory: null,
    use_native_glfw: false,
    use_native_openal: false,
    ...overrides,
  } as ResolvedInstanceSettings;
}

describe("instance settings groups", () => {
  it("maps every field to exactly one card group", () => {
    expect(groupForField("max_memory_mb")).toContain("max_memory_mb");
    expect(groupForField("width")).toEqual(expect.arrayContaining(["window_type", "height"]));
    expect(groupForField("quick_play_singleplayer")).toContain("quick_play");
    expect(groupForField("isolation")).toBeNull();
  });

  it("adds and removes a group without touching unrelated overrides", () => {
    const overrides = ["launcher_visibility", "width"];
    const added = addGroupOverrides(overrides, ["width", "height", "window_type"]);
    expect(added).toEqual(
      expect.arrayContaining(["launcher_visibility", "width", "height", "window_type"])
    );
    expect(isGroupOverridden(added, ["width", "height", "window_type"])).toBe(true);
    expect(removeGroupOverrides(added, ["width", "height", "window_type"])).toEqual([
      "launcher_visibility",
    ]);
  });

  it("serializes nullable fields with the backend clear convention", () => {
    const d = draft({ min_memory_mb: null, metaspace_mb: 256, running_directory: null });
    const patch = buildFieldPatch(d, ["min_memory_mb", "metaspace_mb", "running_directory"]);
    expect(patch).toEqual({ min_memory_mb: 0, metaspace_mb: 256, running_directory: "" });
  });

  it("copies only the requested fields when inheriting again", () => {
    const current = draft({ max_memory_mb: 8192, width: 1920 });
    const effective = draft({ max_memory_mb: 4096, width: 854 });
    const next = pickEffective(current, effective, ["max_memory_mb"]);
    expect(next.max_memory_mb).toBe(4096);
    expect(next.width).toBe(1920);
  });
});

describe("memory helpers", () => {
  it("derives a 512 MiB grid capped at 32 GiB", () => {
    expect(memoryLimits(16 * 1024 ** 3).suggestedMb).toBe(8192);
    expect(memoryLimits(8 * 1024 ** 3).maxMb).toBe(8192);
    expect(memoryLimits(64 * 1024 ** 3).maxMb).toBe(32768);
    expect(memoryLimits(0).maxMb).toBe(32768);
  });

  it("clamps to the slider grid", () => {
    expect(clampMemoryMb(100, 8192)).toBe(512);
    expect(clampMemoryMb(9999, 8192)).toBe(8192);
    expect(clampMemoryMb(4090, 8192)).toBe(4096);
    expect(clampMemoryMb(Number.NaN, 8192)).toBe(512);
  });

  it("formats MiB and GiB readably", () => {
    expect(formatMemoryMb(4096)).toBe("4 GiB");
    expect(formatMemoryMb(6144)).toBe("6 GiB");
    expect(formatMemoryMb(512)).toBe("512 MiB");
  });
});

describe("Java and window helpers", () => {
  it("uses the same Java floors as the launcher", () => {
    expect(requiredJavaFor("1.20.4")).toBe(17);
    expect(requiredJavaFor("1.20.5")).toBe(21);
    expect(requiredJavaFor("1.21.1")).toBe(21);
    expect(requiredJavaFor("1.18.2")).toBe(17);
    expect(requiredJavaFor("1.17.1")).toBe(16);
    expect(requiredJavaFor("1.16.5")).toBe(8);
    expect(requiredJavaFor("26.1")).toBe(21);
  });

  it("matches presets and reports custom resolutions", () => {
    expect(matchingResolution(1920, 1080)?.label).toBe("1920 × 1080");
    expect(matchingResolution(1000, 700)).toBeNull();
    expect(RESOLUTION_PRESETS).toHaveLength(7);
  });
});

describe("environment text", () => {
  it("parses valid lines and ignores broken ones", () => {
    expect(
      parseEnvText("JAVA_HOME=/tmp/jdk\n# comment\nBROKEN\nEMPTY=\nA=B=C\n")
    ).toEqual({ JAVA_HOME: "/tmp/jdk", EMPTY: "", A: "B=C" });
  });
});
