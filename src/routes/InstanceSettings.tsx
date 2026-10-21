import {
  Component,
  For,
  Show,
  createMemo,
  createSignal,
  onMount,
} from "solid-js";
import { useNavigate, useParams } from "@solidjs/router";
import {
  instances,
  mods as modsApi,
  modLoader,
  profiles,
  settings,
  type InstancePaths,
  type JavaInfo,
  type Profile,
} from "../ipc/api";
// `ModList`, `LoaderVersion` and the *full* `ProfileUpdateOpts` (the one that
// carries `override_properties`) only exist in the generated bridge: `../ipc/api`
// re-exports a narrower hand-written `ProfileUpdateOpts` meant for creation.
import type {
  LoaderVersion,
  MissingFileInfo,
  ModList,
  ProfileUpdateOpts,
  ResolvedInstanceSettings,
} from "../ipc/types";
import { showToast } from "../components/Toast";
import { t } from "../i18n";

// ── Overridable fields (rmccc_profile::fields::ALL) ──

/// Every field name accepted in `Profile::override_properties`. The backend
/// filters the list against the same set, so a typo cannot survive a round trip.
const OVERRIDE_FIELDS = [
  "java_path",
  "jvm_args",
  "game_args",
  "memory_mb",
  "min_memory_mb",
  "metaspace_mb",
  "process_priority",
  "width",
  "height",
  "fullscreen",
  "no_generated_jvm_args",
  "no_generated_optimizing_jvm_args",
  "enable_debug_log_output",
  "wrapper",
  "java_agents",
  "override_jvm_args",
  "pre_launch_command",
  "post_exit_command",
  "env_vars",
  "use_custom_natives",
  "use_native_glfw",
  "use_native_openal",
] as const;

type OverrideField = (typeof OVERRIDE_FIELDS)[number];

/** Narrows the generated `string[]` to the known field names. */
const isOverrideField = (name: string): name is OverrideField =>
  (OVERRIDE_FIELDS as readonly string[]).includes(name);

// ── Mod loaders ──

/// The eight loaders, spelled the way `modloader_list` / `modloader_install`
/// expect them (`ModLoaderType` serialises as snake_case, the IPC commands take
/// the library id). Names are brands, so they are not translated.
const LOADER_TYPES: readonly { id: string; label: string }[] = [
  { id: "fabric", label: "Fabric" },
  { id: "forge", label: "Forge" },
  { id: "neoforge", label: "NeoForge" },
  { id: "quilt", label: "Quilt" },
  { id: "liteloader", label: "LiteLoader" },
  { id: "optifine", label: "OptiFine" },
  { id: "cleanroom", label: "Cleanroom" },
  { id: "legacyfabric", label: "Legacy Fabric" },
];

/// `neo_forge` (profile) → `neoforge` (IPC command).
const loaderCommandId = (loaderType: string): string =>
  loaderType.toLowerCase().replaceAll("_", "");

/// The instance's base Minecraft version.
///
/// Installing a loader repoints `profile.game_version` at the patched id
/// (`1.20.4-forge-47.1.0`), but loader APIs are queried by the base id, so drop
/// a trailing `-<loader>-<version>` when the segment after the version is a
/// known loader name. Snapshots and pre-releases (`23w45a`, `1.20.4-pre1`)
/// contain no loader segment and are returned untouched.
function baseGameVersion(versionId: string): string {
  const parts = versionId.split("-");
  const loaderAt = parts.findIndex(
    (part, index) =>
      index > 0 && LOADER_TYPES.some((loader) => loader.id === part.toLowerCase())
  );
  return loaderAt === -1 ? versionId : parts.slice(0, loaderAt).join("-");
}

// ── Text <-> value helpers ──

/** One argument per line, ignoring blank lines (same rule as the Settings page). */
function splitLines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** `KEY=VALUE` per line → the profile's environment map. */
function parseEnvVars(text: string): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    const separator = trimmed.indexOf("=");
    // No separator (or an empty key) means the line cannot be a variable.
    if (separator <= 0) continue;
    vars[trimmed.slice(0, separator).trim()] = trimmed.slice(separator + 1).trim();
  }
  return vars;
}

/** The profile's environment map → the textarea's `KEY=VALUE` lines. */
function formatEnvVars(vars: Profile["env_vars"]): string {
  return Object.entries(vars)
    .map(([key, value]) => `${key}=${value ?? ""}`)
    .join("\n");
}

/** The mods folder implies `.jar`, so the list shows bare names. */
const stripJar = (filename: string): string => filename.replace(/\.jar$/i, "");

// ── Mod row ──

interface ModRowProps {
  filename: string;
  enabled: boolean;
  busy: boolean;
  onToggle: (filename: string, enabled: boolean) => void;
  onRemove: (filename: string) => void;
}

/// One installed mod. The full file name (with `.jar`) is what the backend
/// commands take, so it is passed through untouched.
const ModRow: Component<ModRowProps> = (props) => (
  <div class="java-item">
    <div class="java-info">
      <div class="java-path">{stripJar(props.filename)}</div>
    </div>
    <button
      class="settings-refresh-btn"
      disabled={props.busy}
      onClick={() => props.onToggle(props.filename, !props.enabled)}
    >
      {props.enabled
        ? t("instanceSettings.mods.disable")
        : t("instanceSettings.mods.enable")}
    </button>
    <button
      class="settings-refresh-btn"
      disabled={props.busy}
      onClick={() => props.onRemove(props.filename)}
    >
      {t("instanceSettings.mods.remove")}
    </button>
  </div>
);

// ── Instance settings ──

/// Per-instance settings page (`/instances/:id`).
///
/// Mirrors HMCL's instance settings: every inheritable field shows the
/// instance value plus an inherit/override toggle, and saving sends the
/// *complete* override list together with all instance values — the backend
/// keeps values for fields that currently inherit, so toggling an override off
/// and back on does not lose what the user typed.
const InstanceSettings: Component = () => {
  const navigate = useNavigate();
  const params = useParams();
  const instanceId = () => params.id ?? "";

  // ── Page state ──
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [profile, setProfile] = createSignal<Profile | null>(null);
  const [paths, setPaths] = createSignal<InstancePaths | null>(null);
  /// 生效值（实例覆盖 + 全局默认合并后的结果）。继承字段的提示只读这里，
  /// 前端不再自己实现一遍继承规则（审计 P2-8）。
  const [resolved, setResolved] = createSignal<ResolvedInstanceSettings | null>(null);

  // ── Integrity (0.5.6) ──
  const [missingFiles, setMissingFiles] = createSignal<MissingFileInfo[]>([]);
  const [integrityLoading, setIntegrityLoading] = createSignal(false);

  // ── General ──
  const [name, setName] = createSignal("");

  // ── Inherit / override ──
  const [overrides, setOverrides] = createSignal<OverrideField[]>([]);

  // ── Isolation (instance-only, never part of `override_properties`) ──
  const [isolation, setIsolation] = createSignal("");

  // ── Java ──
  const [javaPath, setJavaPath] = createSignal("");
  const [javaList, setJavaList] = createSignal<JavaInfo[]>([]);

  // ── Memory & arguments ──
  const [memoryMb, setMemoryMb] = createSignal(4096);
  const [minMemoryMb, setMinMemoryMb] = createSignal(256);
  const [metaspaceMb, setMetaspaceMb] = createSignal(0);
  const [jvmArgs, setJvmArgs] = createSignal("");
  const [gameArgs, setGameArgs] = createSignal("");
  const [overrideJvmArgs, setOverrideJvmArgs] = createSignal("");
  const [javaAgents, setJavaAgents] = createSignal("");

  // ── Window & process ──
  const [width, setWidth] = createSignal(0);
  const [height, setHeight] = createSignal(0);
  const [fullscreen, setFullscreen] = createSignal(false);
  const [processPriority, setProcessPriority] = createSignal("normal");

  // ── Mod loader ──
  const [loaderType, setLoaderType] = createSignal(LOADER_TYPES[0].id);
  const [loaderVersions, setLoaderVersions] = createSignal<LoaderVersion[]>([]);
  const [loaderVersion, setLoaderVersion] = createSignal("");
  const [versionState, setVersionState] = createSignal<
    "idle" | "loading" | "ready" | "empty"
  >("idle");
  const [installingLoader, setInstallingLoader] = createSignal(false);

  // ── Mods ──
  const [mods, setMods] = createSignal<ModList | null>(null);
  const [modsLoading, setModsLoading] = createSignal(true);
  const [modsBusy, setModsBusy] = createSignal<string | null>(null);

  // ── Advanced ──
  const [envVars, setEnvVars] = createSignal("");
  const [wrapper, setWrapper] = createSignal("");
  const [preLaunchCommand, setPreLaunchCommand] = createSignal("");
  const [postExitCommand, setPostExitCommand] = createSignal("");
  const [enableDebugLog, setEnableDebugLog] = createSignal(false);
  const [noGeneratedJvmArgs, setNoGeneratedJvmArgs] = createSignal(false);
  const [noOptimizingJvmArgs, setNoOptimizingJvmArgs] = createSignal(false);
  const [customNatives, setCustomNatives] = createSignal(false);
  const [nativeGlfw, setNativeGlfw] = createSignal(false);
  const [nativeOpenal, setNativeOpenal] = createSignal(false);

  // ── Derived ──
  const baseVersion = createMemo(() => baseGameVersion(profile()?.game_version ?? ""));
  const enabledMods = createMemo(() => mods()?.enabled ?? []);
  const disabledMods = createMemo(() => mods()?.disabled ?? []);
  const isOverridden = (field: OverrideField) => overrides().includes(field);

  /// Options for the Java select: auto-detect, every detected installation, and
  /// a hand-typed path that is not in the detected list (otherwise the select
  /// would show "auto-detect" while a custom path is set).
  const javaOptions = createMemo(() => {
    const options = [{ path: "", label: t("instanceSettings.java.auto") }];
    for (const java of javaList()) {
      options.push({
        path: java.path,
        label: `${java.path} — Java ${java.major_version}`,
      });
    }
    const current = javaPath();
    if (current !== "" && !javaList().some((java) => java.path === current)) {
      options.push({ path: current, label: current });
    }
    return options;
  });

  // ── Inherit / override toggle ──

  const toggleOverride = (field: OverrideField) => {
    setOverrides((prev) =>
      prev.includes(field) ? prev.filter((f) => f !== field) : [...prev, field]
    );
  };

  /// 🔓 = takes the global default, 🎚 = takes the instance value.
  const inheritToggle = (field: OverrideField) => (
    <button
      type="button"
      class={`inherit-toggle${isOverridden(field) ? " overridden" : ""}`}
      title={
        isOverridden(field)
          ? t("instanceSettings.overridden")
          : t("instanceSettings.inherit")
      }
      onClick={() => toggleOverride(field)}
    >
      {isOverridden(field) ? "🎚" : "🔓"}
    </button>
  );

  /// 生效值提示：字段处于"继承"状态时，显示后端算出的实际取值。
  /// 返回 `null` 表示没有可显示的值（已覆盖 / 未加载 / 无对应字段）。
  const effectiveHint = (field: OverrideField): string | null => {
    const r = resolved();
    if (!r || isOverridden(field)) return null;
    // 少数后端字段名与 override 名不同（HMCL 沿用 `max_memory_mb`）。
    const key = field === "memory_mb" ? "max_memory_mb" : field;
    const value = (r as unknown as Record<string, unknown>)[key];
    if (value === undefined || value === null || value === "") return null;
    if (Array.isArray(value)) return value.length ? value.join(" ") : null;
    if (typeof value === "boolean") return value ? "true" : "false";
    return String(value);
  };

  /// Field label (with an optional hint) and its inherit toggle on one line.
  const fieldHeader = (label: string, field: OverrideField, hint?: string) => (
    <div class="inherit-row">
      <span class="settings-label">
        {label}
        <Show when={hint}>
          <span class="settings-hint">{hint}</span>
        </Show>
        <Show when={effectiveHint(field)}>
          {(value) => (
            <span class="settings-hint settings-hint-effective">
              {t("instanceSettings.effective", { value: value() })}
            </span>
          )}
        </Show>
      </span>
      {inheritToggle(field)}
    </div>
  );

  /// Checkbox field with its inherit toggle: the box carries the label.
  const checkboxRow = (
    label: string,
    field: OverrideField,
    checked: () => boolean,
    onChange: (value: boolean) => void
  ) => (
    <div class="settings-field">
      <div class="inherit-row">
        <label class="modal-checkbox">
          <input
            type="checkbox"
            checked={checked()}
            onChange={(e) => onChange(e.currentTarget.checked)}
          />
          {label}
        </label>
        {inheritToggle(field)}
      </div>
    </div>
  );

  // ── Data loading ──

  /// Reads the stored profile. `hydrate` copies it into the form signals; the
  /// post-install refresh skips that so unsaved edits survive.
  const loadProfile = async (hydrate: boolean): Promise<Profile | null> => {
    const result = await profiles.list();
    if (!result.ok) {
      showToast(result.error);
      return null;
    }
    const found = result.data.find((p) => p.id === instanceId()) ?? null;
    if (found) {
      setProfile(found);
      if (found.mod_loader) setLoaderType(loaderCommandId(found.mod_loader.loader_type));
      if (hydrate) applyProfile(found);
    }
    return found;
  };

  const applyProfile = (p: Profile) => {
    setName(p.name);
    setIsolation(p.isolation ?? "");
    setOverrides(p.override_properties.filter(isOverrideField));
    setJavaPath(p.java_path ?? "");
    setMemoryMb(p.memory_mb);
    setMinMemoryMb(p.min_memory_mb);
    setMetaspaceMb(p.metaspace_mb);
    setJvmArgs(p.jvm_args.join("\n"));
    setGameArgs(p.game_args.join("\n"));
    setOverrideJvmArgs(p.override_jvm_args.join("\n"));
    setJavaAgents(p.java_agents.join("\n"));
    setWidth(p.width ?? 0);
    setHeight(p.height ?? 0);
    setFullscreen(p.fullscreen);
    setProcessPriority(p.process_priority);
    setEnvVars(formatEnvVars(p.env_vars));
    setWrapper(p.wrapper ?? "");
    setPreLaunchCommand(p.pre_launch_command ?? "");
    setPostExitCommand(p.post_exit_command ?? "");
    setEnableDebugLog(p.enable_debug_log_output);
    setNoGeneratedJvmArgs(p.no_generated_jvm_args);
    setNoOptimizingJvmArgs(p.no_generated_optimizing_jvm_args);
    setCustomNatives(p.use_custom_natives);
    setNativeGlfw(p.use_native_glfw);
    setNativeOpenal(p.use_native_openal);
  };

  /// 生效值只从后端取（审计 P2-8 单一来源）。
  const loadResolved = async () => {
    const result = await instances.resolvedSettings(instanceId());
    if (result.ok) setResolved(result.data);
  };

  const loadPaths = async () => {
    const result = await instances.paths(instanceId());
    if (result.ok) {
      setPaths(result.data);
    } else {
      showToast(result.error);
    }
  };

  const detectJava = async () => {
    const result = await settings.detectJava();
    if (result.ok) {
      setJavaList(result.data);
    } else {
      showToast(result.error);
    }
  };

  const loadMods = async () => {
    setModsLoading(true);
    const result = await modsApi.list(instanceId());
    if (result.ok) {
      setMods(result.data);
    } else {
      showToast(result.error);
    }
    setModsLoading(false);
  };

  const checkIntegrity = async () => {
    setIntegrityLoading(true);
    const r = await instances.checkCompletion(instanceId());
    setIntegrityLoading(false);
    if (r.ok) setMissingFiles(r.data);
    else showToast(r.error);
  };

  const repairInstance = async () => {
    setIntegrityLoading(true);
    const r = await instances.repair(instanceId());
    setIntegrityLoading(false);
    if (r.ok) {
      showToast({ code: "success", message: t("instanceSettings.integrity.repaired"), retryable: false });
      await checkIntegrity();
    } else showToast(r.error);
  };

  onMount(async () => {
    const found = await loadProfile(true);
    setLoading(false);
    if (!found) return;
    await Promise.all([loadPaths(), loadResolved(), detectJava(), loadMods()]);
  });

  // ── Actions ──

  const handleSave = async () => {
    const current = profile();
    if (!current) return;

    setSaving(true);

    const trimmedName = name().trim();
    // Every field is sent, overridden or not: the backend preserves values for
    // fields that inherit, so clearing an override never discards them.
    const opts: ProfileUpdateOpts = {
      name: trimmedName.length > 0 ? trimmedName : null,
      game_version: null,
      icon: null,
      isolation: isolation(),
      override_properties: [...overrides()],
      java_path: javaPath(),
      jvm_args: splitLines(jvmArgs()),
      game_args: splitLines(gameArgs()),
      memory_mb: memoryMb(),
      min_memory_mb: minMemoryMb(),
      metaspace_mb: metaspaceMb(),
      process_priority: processPriority(),
      width: width(),
      height: height(),
      fullscreen: fullscreen(),
      use_custom_natives: customNatives(),
      use_native_glfw: nativeGlfw(),
      use_native_openal: nativeOpenal(),
      no_generated_jvm_args: noGeneratedJvmArgs(),
      no_generated_optimizing_jvm_args: noOptimizingJvmArgs(),
      enable_debug_log_output: enableDebugLog(),
      wrapper: wrapper(),
      java_agents: splitLines(javaAgents()),
      override_jvm_args: splitLines(overrideJvmArgs()),
      pre_launch_command: preLaunchCommand(),
      post_exit_command: postExitCommand(),
      env_vars: parseEnvVars(envVars()),
    };

    const result = await profiles.update(current.id, opts);
    setSaving(false);

    if (result.ok) {
      // The response is the stored truth (isolation normalised, unknown
      // override names dropped), so hydrate from it rather than from the form.
      setProfile(result.data);
      applyProfile(result.data);
      showToast({
        code: "success",
        message: t("instanceSettings.saved"),
        retryable: false,
      });
      // Isolation decides the run directory, so the paths may have moved.
      await loadPaths();
    } else {
      showToast(result.error);
    }
  };

  const loadLoaderVersions = async () => {
    setVersionState("loading");
    setLoaderVersions([]);
    setLoaderVersion("");
    const result = await modLoader.list(loaderType(), baseVersion());
    if (result.ok) {
      setLoaderVersions(result.data);
      if (result.data.length > 0) {
        setLoaderVersion(result.data[0].version);
        setVersionState("ready");
      } else {
        setVersionState("empty");
      }
    } else {
      setVersionState("idle");
      showToast(result.error);
    }
  };

  const installLoader = async () => {
    const version = loaderVersion();
    if (version === "") return;

    setInstallingLoader(true);
    const result = await modLoader.install(instanceId(), loaderType(), version);
    setInstallingLoader(false);

    if (result.ok) {
      // Installing repoints `game_version` at the patched id and sets
      // `mod_loader`; reload without hydrating so unsaved edits survive.
      await loadProfile(false);
      showToast({
        code: "success",
        message: t("instanceSettings.loader.installed"),
        retryable: false,
      });
      // "modded" isolation only isolates once a loader exists, so both the run
      // directory and the mods folder may have moved.
      await Promise.all([loadPaths(), loadMods()]);
    } else {
      showToast(result.error);
    }
  };

  const installLocalMod = async () => {
    const r = await modsApi.pickAndInstall(instanceId());
    if (r.ok) {
      if (r.data) {
        showToast({ code: "success", message: t("instanceSettings.mods.localInstalled"), retryable: false });
        await loadMods();
      }
    } else showToast(r.error);
  };

  const toggleMod = async (filename: string, enabled: boolean) => {
    setModsBusy(filename);
    const result = await modsApi.toggle(instanceId(), filename, enabled);
    setModsBusy(null);
    if (result.ok) {
      showToast({
        code: "success",
        message: t("instanceSettings.mods.toggled"),
        retryable: false,
      });
      await loadMods();
    } else {
      showToast(result.error);
    }
  };

  const removeMod = async (filename: string) => {
    setModsBusy(filename);
    const result = await modsApi.remove(instanceId(), filename);
    setModsBusy(null);
    if (result.ok) {
      showToast({
        code: "success",
        message: t("instanceSettings.mods.removed"),
        retryable: false,
      });
      await loadMods();
    } else {
      showToast(result.error);
    }
  };

  // ── Render ──
  return (
    <div class="settings-page">
      <div class="settings-header">
        <button class="settings-back-btn" onClick={() => navigate("/")}>
          {t("instanceSettings.back")}
        </button>
        <h2 class="settings-title">{t("instanceSettings.title")}</h2>
      </div>

      <Show
        when={!loading()}
        fallback={
          <div class="loading-inline">
            <div class="loading-spinner-small" />
            <span>{t("instanceSettings.loading")}</span>
          </div>
        }
      >
        <Show
          when={profile()}
          fallback={<div class="settings-empty">{t("instanceSettings.notFound")}</div>}
        >
          {(instance) => (
            <div class="settings-content">
              <p class="settings-section-desc">
                {t("instanceSettings.subtitle", { name: instance().name })}
              </p>
              <p class="settings-section-desc">
                {t("instanceSettings.globalDefaultsHint")}
              </p>

              {/* ── General (instance-only, never inheritable) ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.general")}
                </h3>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.field.name")}
                  </label>
                  <input
                    type="text"
                    class="settings-input"
                    value={name()}
                    onInput={(e) => setName(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.field.gameVersion")}
                  </label>
                  <input
                    type="text"
                    class="settings-input settings-input-mono"
                    value={instance().game_version}
                    readonly
                  />
                </div>
              </div>

              {/* ── Isolation (instance-only) ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.isolation")}
                </h3>
                <p class="settings-section-desc">
                  {t("instanceSettings.isolation.subtitle")}
                </p>

                <div class="settings-field">
                  <select
                    class="settings-input"
                    onChange={(e) => setIsolation(e.currentTarget.value)}
                  >
                    {/* `selected` per option rather than `value` on the select:
                        Solid applies attributes before the options exist. */}
                    <option value="" selected={isolation() === ""}>
                      {t("instanceSettings.isolation.inherit")}
                    </option>
                    <option value="never" selected={isolation() === "never"}>
                      {t("instanceSettings.isolation.never")}
                    </option>
                    <option value="modded" selected={isolation() === "modded"}>
                      {t("instanceSettings.isolation.modded")}
                    </option>
                    <option value="always" selected={isolation() === "always"}>
                      {t("instanceSettings.isolation.always")}
                    </option>
                  </select>
                  <span class="settings-hint">
                    {t("instanceSettings.isolation.runDir", {
                      path: paths()?.run_dir ?? "…",
                    })}
                  </span>
                </div>
              </div>

              {/* ── Integrity (0.5.6) ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">{t("instanceSettings.integrity.title")}</h3>
                <p class="settings-section-desc">{t("instanceSettings.integrity.desc")}</p>
                <div class="modal-actions">
                  <button class="modal-btn-primary" disabled={integrityLoading()} onClick={checkIntegrity}>
                    {integrityLoading() ? t("settings.loading") : t("instanceSettings.integrity.check")}
                  </button>
                  <button class="modal-btn-cancel" disabled={integrityLoading()} onClick={repairInstance}>
                    {t("instanceSettings.integrity.repair")}
                  </button>
                </div>
                <Show when={missingFiles().length > 0}>
                  <div class="settings-hint">
                    {t("instanceSettings.integrity.missing", { count: missingFiles().length })}
                  </div>
                </Show>
              </div>

              {/* ── Java ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.java")}
                </h3>

                <div class="settings-field">
                  {fieldHeader(t("instanceSettings.java.detected"), "java_path")}
                  <select
                    class="settings-input"
                    onChange={(e) => setJavaPath(e.currentTarget.value)}
                  >
                    <For each={javaOptions()}>
                      {(option) => (
                        <option
                          value={option.path}
                          selected={javaPath() === option.path}
                        >
                          {option.label}
                        </option>
                      )}
                    </For>
                  </select>
                </div>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.java.path")}
                  </label>
                  <input
                    type="text"
                    class="settings-input settings-input-mono"
                    value={javaPath()}
                    onInput={(e) => setJavaPath(e.currentTarget.value)}
                  />
                </div>
              </div>

              {/* ── Memory & arguments ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.memory")}
                </h3>

                <div class="memory-row">
                  <div class="settings-field memory-field">
                    {fieldHeader(
                      t("instanceSettings.memory.max"),
                      "memory_mb",
                      t("instanceSettings.args.unit")
                    )}
                    <input
                      type="number"
                      class="settings-input"
                      min="256"
                      step="256"
                      value={memoryMb()}
                      onInput={(e) => setMemoryMb(Number(e.currentTarget.value))}
                    />
                  </div>
                  <div class="settings-field memory-field">
                    {fieldHeader(
                      t("instanceSettings.memory.min"),
                      "min_memory_mb",
                      t("instanceSettings.args.unit")
                    )}
                    <input
                      type="number"
                      class="settings-input"
                      min="128"
                      step="128"
                      value={minMemoryMb()}
                      onInput={(e) => setMinMemoryMb(Number(e.currentTarget.value))}
                    />
                  </div>
                  <div class="settings-field memory-field">
                    {fieldHeader(
                      t("instanceSettings.memory.metaspace"),
                      "metaspace_mb",
                      t("instanceSettings.args.unit")
                    )}
                    <input
                      type="number"
                      class="settings-input"
                      min="0"
                      step="64"
                      value={metaspaceMb()}
                      onInput={(e) => setMetaspaceMb(Number(e.currentTarget.value))}
                    />
                  </div>
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.args.jvm"),
                    "jvm_args",
                    t("instanceSettings.args.onePerLine")
                  )}
                  <textarea
                    class="settings-input settings-input-mono"
                    rows="3"
                    value={jvmArgs()}
                    onInput={(e) => setJvmArgs(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.args.game"),
                    "game_args",
                    t("instanceSettings.args.onePerLine")
                  )}
                  <textarea
                    class="settings-input settings-input-mono"
                    rows="3"
                    value={gameArgs()}
                    onInput={(e) => setGameArgs(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.args.overrideJvm"),
                    "override_jvm_args",
                    t("instanceSettings.args.onePerLine")
                  )}
                  <textarea
                    class="settings-input settings-input-mono"
                    rows="3"
                    value={overrideJvmArgs()}
                    onInput={(e) => setOverrideJvmArgs(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.args.agents"),
                    "java_agents",
                    t("instanceSettings.args.onePerLine")
                  )}
                  <textarea
                    class="settings-input settings-input-mono"
                    rows="3"
                    value={javaAgents()}
                    onInput={(e) => setJavaAgents(e.currentTarget.value)}
                  />
                </div>
              </div>

              {/* ── Window & process ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.window")}
                </h3>

                <div class="memory-row">
                  <div class="settings-field memory-field">
                    {fieldHeader(t("instanceSettings.window.width"), "width")}
                    <input
                      type="number"
                      class="settings-input"
                      min="0"
                      value={width()}
                      onInput={(e) => setWidth(Number(e.currentTarget.value))}
                    />
                  </div>
                  <div class="settings-field memory-field">
                    {fieldHeader(t("instanceSettings.window.height"), "height")}
                    <input
                      type="number"
                      class="settings-input"
                      min="0"
                      value={height()}
                      onInput={(e) => setHeight(Number(e.currentTarget.value))}
                    />
                  </div>
                </div>

                {checkboxRow(
                  t("instanceSettings.window.fullscreen"),
                  "fullscreen",
                  fullscreen,
                  setFullscreen
                )}

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.window.priority"),
                    "process_priority"
                  )}
                  <select
                    class="settings-input"
                    onChange={(e) => setProcessPriority(e.currentTarget.value)}
                  >
                    <option
                      value="low"
                      selected={processPriority() === "low"}
                    >
                      {t("instanceSettings.window.priority.low")}
                    </option>
                    <option
                      value="below_normal"
                      selected={processPriority() === "below_normal"}
                    >
                      {t("instanceSettings.window.priority.below_normal")}
                    </option>
                    <option
                      value="normal"
                      selected={processPriority() === "normal"}
                    >
                      {t("instanceSettings.window.priority.normal")}
                    </option>
                    <option
                      value="above_normal"
                      selected={processPriority() === "above_normal"}
                    >
                      {t("instanceSettings.window.priority.above_normal")}
                    </option>
                    <option
                      value="high"
                      selected={processPriority() === "high"}
                    >
                      {t("instanceSettings.window.priority.high")}
                    </option>
                  </select>
                </div>
              </div>

              {/* ── Mod loader ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.loader")}
                  <button
                    class="settings-refresh-btn"
                    disabled={versionState() === "loading"}
                    onClick={loadLoaderVersions}
                  >
                    {versionState() === "loading"
                      ? t("instanceSettings.loader.loading")
                      : t("instanceSettings.loader.loadVersions")}
                  </button>
                </h3>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.loader.current")}
                  </label>
                  <Show
                    when={instance().mod_loader}
                    fallback={
                      <span class="settings-hint">
                        {t("instanceSettings.loader.none")}
                      </span>
                    }
                  >
                    {(loader) => (
                      <span class="settings-hint">
                        {`${loader().loader_type} ${loader().loader_version}`}
                      </span>
                    )}
                  </Show>
                </div>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.loader.select")}
                  </label>
                  <select
                    class="settings-input"
                    onChange={(e) => {
                      setLoaderType(e.currentTarget.value);
                      // A version list belongs to one loader, so drop it.
                      setLoaderVersions([]);
                      setLoaderVersion("");
                      setVersionState("idle");
                    }}
                  >
                    <For each={LOADER_TYPES}>
                      {(loader) => (
                        <option
                          value={loader.id}
                          selected={loaderType() === loader.id}
                        >
                          {loader.label}
                        </option>
                      )}
                    </For>
                  </select>
                </div>

                <div class="settings-field">
                  <label class="settings-label">
                    {t("instanceSettings.loader.version")}
                    <span class="settings-hint">{baseVersion()}</span>
                  </label>
                  <Show when={versionState() === "loading"}>
                    <div class="loading-inline">
                      <div class="loading-spinner-small" />
                      <span>{t("instanceSettings.loader.loading")}</span>
                    </div>
                  </Show>
                  <Show when={versionState() === "empty"}>
                    <div class="settings-empty">
                      {t("instanceSettings.loader.noneAvailable")}
                    </div>
                  </Show>
                  <Show when={versionState() === "ready"}>
                    <select
                      class="settings-input"
                      onChange={(e) => setLoaderVersion(e.currentTarget.value)}
                    >
                      <For each={loaderVersions()}>
                        {(v) => (
                          <option
                            value={v.version}
                            selected={loaderVersion() === v.version}
                          >
                            {/* Stability marker: ★ stable, β pre-release. */}
                            <span
                              title={
                                v.stable
                                  ? t("instanceSettings.loader.stable")
                                  : t("instanceSettings.loader.prerelease")
                              }
                            >
                              {`${v.version} ${v.stable ? "★" : "β"}`}
                            </span>
                          </option>
                        )}
                      </For>
                    </select>
                  </Show>
                </div>

                <div class="settings-field">
                  <button
                    class="settings-save-btn"
                    disabled={
                      installingLoader() || versionState() !== "ready"
                    }
                    onClick={installLoader}
                  >
                    {installingLoader()
                      ? t("instanceSettings.loader.installing")
                      : t("instanceSettings.loader.install")}
                  </button>
                </div>
              </div>

              {/* ── Mods ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.mods")}
                </h3>
                <p class="settings-section-desc">
                  {t("instanceSettings.mods.folder", {
                    path: mods()?.mods_dir ?? paths()?.mods_dir ?? "…",
                  })}
                </p>

                <Show
                  when={!modsLoading()}
                  fallback={
                    <div class="loading-inline">
                      <div class="loading-spinner-small" />
                      <span>{t("instanceSettings.mods.loading")}</span>
                    </div>
                  }
                >
                  <Show
                    when={enabledMods().length + disabledMods().length > 0}
                    fallback={
                      <div class="settings-empty">
                        {t("instanceSettings.mods.empty")}
                      </div>
                    }
                  >
                    <Show when={enabledMods().length > 0}>
                      <div class="settings-label">
                        {t("instanceSettings.mods.enabled")}
                      </div>
                      <div class="java-list">
                        <For each={enabledMods()}>
                          {(filename) => (
                            <ModRow
                              filename={filename}
                              enabled
                              busy={modsBusy() === filename}
                              onToggle={toggleMod}
                              onRemove={removeMod}
                            />
                          )}
                        </For>
                      </div>
                    </Show>

                    <Show when={disabledMods().length > 0}>
                      <div class="settings-label" style="margin-top: 12px">
                        {t("instanceSettings.mods.disabled")}
                      </div>
                      <div class="java-list">
                        <For each={disabledMods()}>
                          {(filename) => (
                            <ModRow
                              filename={filename}
                              enabled={false}
                              busy={modsBusy() === filename}
                              onToggle={toggleMod}
                              onRemove={removeMod}
                            />
                          )}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </Show>

                <div class="modal-actions">
                  <button class="modal-btn-primary" onClick={installLocalMod}>
                    {t("instanceSettings.mods.installLocal")}
                  </button>
                </div>
              </div>

              {/* ── Advanced ── */}
              <div class="settings-section">
                <h3 class="settings-section-title">
                  {t("instanceSettings.section.advanced")}
                </h3>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.advanced.envVars"),
                    "env_vars",
                    t("instanceSettings.advanced.envVars.hint")
                  )}
                  <textarea
                    class="settings-input settings-input-mono"
                    rows="3"
                    value={envVars()}
                    onInput={(e) => setEnvVars(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(t("instanceSettings.advanced.wrapper"), "wrapper")}
                  <input
                    type="text"
                    class="settings-input settings-input-mono"
                    value={wrapper()}
                    onInput={(e) => setWrapper(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.advanced.preLaunch"),
                    "pre_launch_command"
                  )}
                  <input
                    type="text"
                    class="settings-input settings-input-mono"
                    value={preLaunchCommand()}
                    onInput={(e) => setPreLaunchCommand(e.currentTarget.value)}
                  />
                </div>

                <div class="settings-field">
                  {fieldHeader(
                    t("instanceSettings.advanced.postExit"),
                    "post_exit_command"
                  )}
                  <input
                    type="text"
                    class="settings-input settings-input-mono"
                    value={postExitCommand()}
                    onInput={(e) => setPostExitCommand(e.currentTarget.value)}
                  />
                </div>

                {checkboxRow(
                  t("instanceSettings.advanced.debugLog"),
                  "enable_debug_log_output",
                  enableDebugLog,
                  setEnableDebugLog
                )}
                {checkboxRow(
                  t("instanceSettings.advanced.noGeneratedJvmArgs"),
                  "no_generated_jvm_args",
                  noGeneratedJvmArgs,
                  setNoGeneratedJvmArgs
                )}
                {checkboxRow(
                  t("instanceSettings.advanced.noOptimizingJvmArgs"),
                  "no_generated_optimizing_jvm_args",
                  noOptimizingJvmArgs,
                  setNoOptimizingJvmArgs
                )}
                {checkboxRow(
                  t("instanceSettings.advanced.customNatives"),
                  "use_custom_natives",
                  customNatives,
                  setCustomNatives
                )}
                {checkboxRow(
                  t("instanceSettings.advanced.nativeGlfw"),
                  "use_native_glfw",
                  nativeGlfw,
                  setNativeGlfw
                )}
                {checkboxRow(
                  t("instanceSettings.advanced.nativeOpenal"),
                  "use_native_openal",
                  nativeOpenal,
                  setNativeOpenal
                )}
              </div>

              {/* ── Save ── */}
              <div class="settings-actions">
                <button
                  class="settings-save-btn"
                  disabled={saving()}
                  onClick={handleSave}
                >
                  {saving()
                    ? t("instanceSettings.saving")
                    : t("instanceSettings.save")}
                </button>
              </div>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};

export default InstanceSettings;
