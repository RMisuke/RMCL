/**
 * Instance settings (rewritten 2026-10-31).
 *
 * Reference points from HMCL's GameSettingsPage:
 * - a vertical list of titled sections, each built from compact rows;
 * - label on the left, editor on the right, full-width editor only for long text;
 * - one inheritance lock per setting group, not one dead control per field;
 * - Java is a mode switch plus an inline editor, memory is slider + number +
 *   physical-memory bar, window size is a preset dropdown + exact numbers.
 *
 * RMCL-specific decisions kept from the design system: card surfaces, accent
 * group locks, progressive disclosure for JVM/args/natives/debug, and a
 * sticky auto-save status. Every control here maps to a backend field that the
 * launcher actually consumes.
 */
import type { JSX } from "solid-js";
import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from "solid-js";
import {
  RmButton,
  RmInput,
  RmSegmented,
  RmSelect,
  RmSlider,
  RmSwitch,
  RmTag,
} from "../rm/controls";
import { RmListRow } from "../rm/surfaces";
import { Icon } from "../icons";
import {
  MemoryMeter,
  NumberField,
  SettingsCard,
  SettingsRow,
  SettingsSection,
} from "./primitives";
import {
  instances,
  dialog,
  settings as settingsApi,
  type InstanceSettingsPatchInput,
} from "../../ipc/api";
import type {
  InstanceDetail,
  InstanceIcon,
  InstancePaths,
  JavaInfo,
  MissingFileInfo,
  ResolvedInstanceSettings,
  SystemMemoryInfo,
} from "../../ipc/types";
import { instancesStore } from "../../stores/instances";
import { launchStore } from "../../stores/launch";
import { toast } from "../../stores/ui";
import { t } from "../../i18n";
import { iconUrl } from "../../lib/instanceView";
import {
  AGENT_FIELDS,
  GAME_FIELDS,
  GROUP_FIELDS,
  JAVA_FIELDS,
  JVM_FIELDS,
  QUICK_PLAY_FIELDS,
  RESOLUTION_PRESETS,
  WINDOW_FIELDS,
  addGroupOverrides,
  buildFieldPatch,
  clampMemoryMb,
  formatEnvText,
  formatMemoryMb,
  groupForField,
  isGroupOverridden,
  javaRuntimeLabel,
  matchingResolution,
  memoryLimits,
  parseEnvText,
  pickEffective,
  removeGroupOverrides,
  requiredJavaFor,
  resolutionKey,
  type SettingGroupId,
} from "../../lib/instanceSettings";

const ICON_MAX_BYTES = 512 * 1024;

function splitLines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function InstanceSettings(props: { detail: InstanceDetail }): JSX.Element {
  const key = () => props.detail.summary.key;
  const initial = () => props.detail.effective as unknown as ResolvedInstanceSettings;

  const [draft, setDraft] = createSignal<ResolvedInstanceSettings>(initial());
  const [overrides, setOverrides] = createSignal<string[]>([
    ...props.detail.settings.override_properties,
  ]);
  const [javas, setJavas] = createSignal<JavaInfo[]>([]);
  const [memoryInfo, setMemoryInfo] = createSignal<SystemMemoryInfo | null>(null);
  const [paths, setPaths] = createSignal<InstancePaths>(props.detail.paths);
  const [advancedOpen, setAdvancedOpen] = createSignal(false);

  const [name, setName] = createSignal(props.detail.settings.display_name ?? "");
  const [icon, setIcon] = createSignal<InstanceIcon>(props.detail.settings.icon);
  const [isolation, setIsolation] = createSignal<string>(props.detail.settings.isolation ?? "");

  const [jvmText, setJvmText] = createSignal(initial().jvm_args.join("\n"));
  const [gameArgsText, setGameArgsText] = createSignal(initial().game_args.join("\n"));
  const [agentsText, setAgentsText] = createSignal(initial().java_agents.join("\n"));
  const [envText, setEnvText] = createSignal(formatEnvText(initial().env_vars));

  const [saving, setSaving] = createSignal(false);
  const [savedAt, setSavedAt] = createSignal<number | null>(null);
  const [saveError, setSaveError] = createSignal<string | null>(null);

  const [missing, setMissing] = createSignal<MissingFileInfo[] | null>(null);
  const [checking, setChecking] = createSignal(false);
  const [repairing, setRepairing] = createSignal(false);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingPatch: InstanceSettingsPatchInput = {};
  let pendingSync: string[] | null = null;
  let inFlight = false;
  let flushAgain = false;

  // ── Auto-save queue ─────────────────────────────────────────────────────

  function queue(
    patch: InstanceSettingsPatchInput,
    opts: { immediate?: boolean; syncFields?: readonly string[] } = {}
  ): void {
    pendingPatch = { ...pendingPatch, ...patch };
    pendingSync = opts.syncFields ? [...opts.syncFields] : null;
    setSavedAt(null);
    setSaveError(null);
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = undefined;
    }
    if (opts.immediate) {
      void flush();
    } else {
      saveTimer = setTimeout(() => {
        saveTimer = undefined;
        void flush();
      }, 420);
    }
  }

  async function flush(): Promise<void> {
    if (inFlight) {
      flushAgain = true;
      return;
    }
    if (Object.keys(pendingPatch).length === 0) return;
    const patch = pendingPatch;
    const sync = pendingSync;
    pendingPatch = {};
    pendingSync = null;
    inFlight = true;
    setSaving(true);
    const res = await instances.updateSettings(key(), patch);
    if (res.ok) {
      instancesStore.upsert(res.data);
      if (sync) {
        const effective = res.data.effective as unknown as ResolvedInstanceSettings;
        setDraft((current) => pickEffective(current, effective, sync));
        syncTextEditors(effective, sync);
      }
      if ("isolation" in patch) {
        setIsolation(res.data.settings.isolation ?? "");
        setPaths(res.data.paths);
      }
      if ("icon" in patch) setIcon(res.data.settings.icon);
      setSavedAt(Date.now());
      setSaveError(null);
    } else {
      setSaveError(res.error.message);
      toast({ message: res.error.message, tone: "error" });
    }
    inFlight = false;
    setSaving(false);
    if (flushAgain || Object.keys(pendingPatch).length > 0) {
      flushAgain = false;
      void flush();
    }
  }

  function syncTextEditors(effective: ResolvedInstanceSettings, fields: readonly string[]): void {
    if (fields.includes("jvm_args")) setJvmText(effective.jvm_args.join("\n"));
    if (fields.includes("game_args")) setGameArgsText(effective.game_args.join("\n"));
    if (fields.includes("env_vars")) setEnvText(formatEnvText(effective.env_vars));
    if (fields.includes("java_agents")) setAgentsText(effective.java_agents.join("\n"));
  }

  /** Edit one field, freezing its whole card group into overrides first. */
  function commitField(
    field: string,
    value: unknown,
    opts: { immediate?: boolean } = {}
  ): void {
    const fields = groupForField(field);
    if (!fields) return;
    commitGroup(fields, { [field]: value } as Partial<ResolvedInstanceSettings>, opts);
  }

  function commitGroup(
    fields: readonly string[],
    values: Partial<ResolvedInstanceSettings>,
    opts: { immediate?: boolean } = {}
  ): void {
    const nextDraft = { ...draft(), ...values } as ResolvedInstanceSettings;
    const nextOverrides = addGroupOverrides(overrides(), fields);
    setDraft(nextDraft);
    setOverrides(nextOverrides);
    queue(
      {
        ...(buildFieldPatch(nextDraft, fields) as InstanceSettingsPatchInput),
        override_properties: nextOverrides,
      },
      { immediate: opts.immediate }
    );
  }

  function groupLock(group: SettingGroupId): {
    overridden: boolean;
    onToggle: () => void;
    label: string;
  } {
    const fields = GROUP_FIELDS[group];
    return {
      overridden: isGroupOverridden(overrides(), fields),
      onToggle: () => toggleGroup(group),
      label: isGroupOverridden(overrides(), fields)
        ? t("instanceDetail.settings.group.overridden")
        : t("instanceDetail.settings.group.inherited"),
    };
  }

  function toggleGroup(group: SettingGroupId): void {
    const fields = GROUP_FIELDS[group];
    const on = !isGroupOverridden(overrides(), fields);
    const nextOverrides = on
      ? addGroupOverrides(overrides(), fields)
      : removeGroupOverrides(overrides(), fields);
    setOverrides(nextOverrides);
    if (on) {
      // Freeze the effective values the user is currently looking at.
      queue(
        {
          ...(buildFieldPatch(draft(), fields) as InstanceSettingsPatchInput),
          override_properties: nextOverrides,
        },
        { immediate: true }
      );
    } else {
      queue(
        { override_properties: nextOverrides },
        { immediate: true, syncFields: fields }
      );
    }
  }

  async function savePlain(patch: InstanceSettingsPatchInput, immediate = true): Promise<void> {
    queue(patch, { immediate });
  }

  // ── Lifecycle / data ────────────────────────────────────────────────────

  onMount(() => {
    void settingsApi.detectJava().then((r) => {
      if (r.ok) setJavas(r.data);
    });
    void settingsApi.systemMemory().then((r) => {
      if (r.ok) setMemoryInfo(r.data);
    });
    void instances.paths(key()).then((r) => {
      if (r.ok) setPaths(r.data);
    });
  });

  onCleanup(() => {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = undefined;
    }
    // Flush the last debounced edit when switching tabs. The request keeps
    // running after unmount; the store update is still applied.
    if (Object.keys(pendingPatch).length > 0) void flush();
  });

  createEffect(
    on(
      () => props.detail.summary.key,
      () => {
        const next = props.detail.effective as unknown as ResolvedInstanceSettings;
        setDraft(next);
        setOverrides([...props.detail.settings.override_properties]);
        setPaths(props.detail.paths);
        setName(props.detail.settings.display_name ?? "");
        setIcon(props.detail.settings.icon);
        setIsolation(props.detail.settings.isolation ?? "");
        setJvmText(next.jvm_args.join("\n"));
        setGameArgsText(next.game_args.join("\n"));
        setAgentsText(next.java_agents.join("\n"));
        setEnvText(formatEnvText(next.env_vars));
        setMissing(null);
      },
      { defer: true }
    )
  );

  // ── Derived values ──────────────────────────────────────────────────────

  const limits = createMemo(() => memoryLimits(memoryInfo()?.total_bytes ?? 0));
  const memoryMax = () =>
    Math.max(limits().maxMb, Math.min(32768, draft().max_memory_mb));
  const memoryPercent = () =>
    limits().totalMb > 0
      ? Math.min(100, (draft().max_memory_mb / limits().totalMb) * 100)
      : 0;
  const requiredJava = createMemo(() =>
    requiredJavaFor(props.detail.summary.game_version)
  );

  const javaMode = createMemo<"auto" | "version" | "detected" | "custom">(() => {
    const value = draft();
    if (value.java_type === "auto") return "auto";
    if (value.java_type === "version") return "version";
    if (value.java_type === "detected") return "detected";
    return "custom";
  });

  const selectedDetectedJava = createMemo<JavaInfo | undefined>(() => {
    const value = draft();
    if (value.java_type !== "detected") return undefined;
    return (
      javas().find((java) => java.path === value.custom_java_path) ??
      javas().find((java) => String(java.major_version) === value.detected_java_version)
    );
  });

  const javaWarning = createMemo<string | null>(() => {
    const mode = javaMode();
    if (mode === "auto") return null;
    if (mode === "custom" && !draft().custom_java_path.trim()) {
      return t("instanceDetail.settings.java.missingPath");
    }
    let major: number | undefined;
    if (mode === "version") {
      major = Number(draft().custom_java_version) || undefined;
    } else {
      major = selectedDetectedJava()?.major_version;
    }
    if (major !== undefined && major < requiredJava()) {
      return t("instanceDetail.settings.java.tooOld", { need: requiredJava() });
    }
    return null;
  });

  const resolution = createMemo(() =>
    matchingResolution(draft().width, draft().height)
  );
  const resolutionValue = () =>
    resolution() ? resolutionKey(draft().width, draft().height) : "custom";
  const quickPlayTarget = () => {
    switch (draft().quick_play) {
      case "multiplayer":
        return {
          field: "quick_play_multiplayer",
          label: t("instanceDetail.settings.quickPlay.multiplayerLabel"),
          placeholder: "play.example.com",
        };
      case "singleplayer":
        return {
          field: "quick_play_singleplayer",
          label: t("instanceDetail.settings.quickPlay.singleplayerLabel"),
          placeholder: "New World",
        };
      case "realms":
        return {
          field: "quick_play_realms",
          label: t("instanceDetail.settings.quickPlay.realmsLabel"),
          placeholder: "Realms ID",
        };
      default:
        return null;
    }
  };
  const envInvalid = createMemo(() => {
    const lines = envText().split("\n");
    let invalid = 0;
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const split = trimmed.indexOf("=");
      if (split <= 0) invalid += 1;
    }
    return invalid;
  });

  // ── Identity ────────────────────────────────────────────────────────────

  let iconInput: HTMLInputElement | undefined;

  function saveName(value: string): void {
    setName(value);
    void savePlain({ display_name: value }, false);
  }

  function applyIcon(next: InstanceIcon): void {
    setIcon(next);
    void savePlain({ icon: next });
  }

  function onPickIcon(files: FileList | null): void {
    const file = files?.[0];
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      toast({ message: t("instanceDetail.settings.icon.notImage"), tone: "error" });
      return;
    }
    if (file.size > ICON_MAX_BYTES) {
      toast({ message: t("instanceDetail.settings.icon.tooLarge"), tone: "error" });
      return;
    }
    const reader = new FileReader();
    reader.onload = () => applyIcon({ type: "data_url", value: String(reader.result) });
    reader.onerror = () =>
      toast({ message: t("instanceDetail.settings.icon.readFail"), tone: "error" });
    reader.readAsDataURL(file);
    if (iconInput) iconInput.value = "";
  }

  async function reveal(path: "run" | "instance"): Promise<void> {
    const res = await instances.reveal(
      key(),
      path === "run" ? "run" : "instance"
    );
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  // ── Java ────────────────────────────────────────────────────────────────

  function setJavaMode(mode: "auto" | "version" | "detected" | "custom"): void {
    if (mode === "auto") {
      commitGroup(JAVA_FIELDS, { java_type: "auto" }, { immediate: true });
      return;
    }
    if (mode === "version") {
      commitGroup(
        JAVA_FIELDS,
        {
          java_type: "version",
          custom_java_version:
            draft().custom_java_version || String(requiredJava()),
        },
        { immediate: true }
      );
      return;
    }
    if (mode === "custom") {
      commitGroup(JAVA_FIELDS, { java_type: "custom" }, { immediate: true });
      return;
    }
    const picked =
      javas().find((java) => java.major_version >= requiredJava()) ?? javas()[0];
    if (!picked) {
      toast({ message: t("instanceDetail.settings.java.noDetected"), tone: "error" });
      return;
    }
    chooseDetectedJava(picked.path);
  }

  function chooseDetectedJava(path: string): void {
    const picked = javas().find((java) => java.path === path);
    if (!picked) return;
    commitGroup(
      JAVA_FIELDS,
      {
        java_type: "detected",
        custom_java_path: picked.path,
        detected_java_version: String(picked.major_version),
        detected_java_path_hash: "",
      },
      { immediate: true }
    );
  }

  async function pickJavaDirectory(): Promise<void> {
    const res = await dialog.pickDirectory();
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    if (res.data) {
      commitGroup(
        JAVA_FIELDS,
        { java_type: "custom", custom_java_path: res.data },
        { immediate: true }
      );
    }
  }

  // ── Memory ──────────────────────────────────────────────────────────────

  function setMemory(value: number): void {
    // Debounced: dragging the slider queues many values, only the last one is
    // sent. The knob and number field still update on every input.
    commitField("max_memory_mb", clampMemoryMb(value, memoryMax()));
  }

  // ── Window ──────────────────────────────────────────────────────────────

  function setResolutionPreset(value: string): void {
    if (value === "custom") return;
    const preset = RESOLUTION_PRESETS.find(
      (item) => resolutionKey(item.width, item.height) === value
    );
    if (!preset) return;
    commitGroup(
      WINDOW_FIELDS,
      { width: preset.width, height: preset.height },
      { immediate: true }
    );
  }

  // ── Quick play ──────────────────────────────────────────────────────────

  function setQuickPlay(value: string): void {
    commitGroup(
      QUICK_PLAY_FIELDS,
      { quick_play: value as ResolvedInstanceSettings["quick_play"] },
      { immediate: true }
    );
  }

  function setQuickTarget(field: string, value: string): void {
    commitGroup(QUICK_PLAY_FIELDS, {
      [field]: value,
    } as Partial<ResolvedInstanceSettings>);
  }

  // ── Text editors ────────────────────────────────────────────────────────

  function setArgsText(
    fields: readonly string[],
    field: "jvm_args" | "game_args" | "java_agents",
    text: string
  ): void {
    commitGroup(fields, { [field]: splitLines(text) } as Partial<ResolvedInstanceSettings>);
  }

  function setEnv(value: string): void {
    setEnvText(value);
    commitGroup(GAME_FIELDS, { env_vars: parseEnvText(value) });
  }

  // ── Integrity / isolation ───────────────────────────────────────────────

  async function changeIsolation(value: string): Promise<void> {
    setIsolation(value);
    queue({ isolation: value }, { immediate: true });
  }

  async function checkCompletion(): Promise<void> {
    setChecking(true);
    const res = await instances.checkCompletion(key());
    setChecking(false);
    if (res.ok) setMissing(res.data);
    else toast({ message: res.error.message, tone: "error" });
  }

  async function repairInstance(): Promise<void> {
    setRepairing(true);
    const res = await instances.repair(key());
    setRepairing(false);
    if (res.ok) {
      toast({
        message: t("instanceDetail.maintenance.repaired", { n: res.data }),
        tone: "success",
      });
      void checkCompletion();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  // ── Render helpers ──────────────────────────────────────────────────────

  const groupCard = (group: SettingGroupId) => groupLock(group);

  function isolationLabel(value: string): string {
    switch (value) {
      case "never":
        return t("instanceDetail.maintenance.isolationShared");
      case "always":
        return t("instanceDetail.maintenance.isolationIsolated");
      default:
        return t("instanceDetail.maintenance.isolationModded");
    }
  }

  return (
    <div class="instance-settings">
      <Show when={props.detail.summary.settings_read_only}>
        <div class="is-page-warning">
          <Icon name="alert" size={14} />
          <span>{t("instanceDetail.settings.readOnly")}</span>
        </div>
      </Show>

      <SettingsSection
        icon="package"
        title={t("instanceDetail.settings.section.basic")}
        hint={t("instanceDetail.settings.section.basicHint")}
      >
        <SettingsCard
          title={t("instanceDetail.settings.card.instance")}
          hint={t("instanceDetail.settings.card.instanceHint")}
        >
          <div class="is-identity">
            <div class="is-identity-icon">
              <Show
                when={iconUrl(icon())}
                fallback={
                  <span class="is-identity-placeholder">
                    <Icon name="package" size={26} />
                  </span>
                }
              >
                {(src) => <img src={src()} alt="" />}
              </Show>
            </div>
            <div class="is-identity-main">
              <input
                class="is-identity-name"
                value={name()}
                aria-label={t("instanceDetail.settings.name")}
                placeholder={props.detail.summary.id}
                onInput={(e) => saveName(e.currentTarget.value)}
                onBlur={() => void flush()}
              />
              <div class="is-identity-meta">
                <RmTag tone="accent">{props.detail.summary.game_version}</RmTag>
                <Show when={props.detail.summary.loader}>
                  {(loader) => (
                    <RmTag tone="accent">
                      {loader().kind} {loader().version ?? ""}
                    </RmTag>
                  )}
                </Show>
                <RmTag>{isolationLabel(props.detail.summary.isolation)}</RmTag>
              </div>
              <button
                type="button"
                class="is-path-link"
                title={props.detail.paths.instance_dir}
                onClick={() => void reveal("instance")}
              >
                <Icon name="folder" size={13} />
                <span>{props.detail.paths.instance_dir}</span>
              </button>
            </div>
            <div class="is-identity-actions">
              <input
                ref={iconInput}
                type="file"
                accept="image/*"
                class="visually-hidden"
                onChange={(e) => onPickIcon(e.currentTarget.files)}
              />
              <RmButton size="sm" icon="image" onClick={() => iconInput?.click()}>
                {t("instanceDetail.settings.icon.replace")}
              </RmButton>
              <Show when={icon().type !== "auto"}>
                <RmButton
                  size="sm"
                  variant="ghost"
                  icon="trash"
                  onClick={() => applyIcon({ type: "auto" })}
                >
                  {t("instanceDetail.settings.icon.clear")}
                </RmButton>
              </Show>
            </div>
          </div>
          <div class="is-identity-hint">{t("instanceDetail.settings.icon.hint")}</div>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection
        icon="gamepad"
        title={t("instanceDetail.settings.section.game")}
        hint={t("instanceDetail.settings.section.gameHint")}
      >
        <SettingsCard
          title={t("instanceDetail.settings.card.java")}
          hint={t("instanceDetail.settings.card.javaHint")}
          group={groupCard("java")}
        >
          <SettingsRow
            title={t("instanceDetail.settings.java.mode")}
            hint={t("instanceDetail.settings.java.modeHint")}
          >
            <RmSegmented
              value={javaMode()}
              label={t("instanceDetail.settings.java.mode")}
              options={[
                { value: "auto", label: t("instanceDetail.settings.java.auto") },
                { value: "version", label: t("instanceDetail.settings.java.version") },
                { value: "detected", label: t("instanceDetail.settings.java.detected") },
                { value: "custom", label: t("instanceDetail.settings.java.custom") },
              ]}
              onChange={(value) =>
                setJavaMode(value as "auto" | "version" | "detected" | "custom")
              }
            />
          </SettingsRow>
          <Show when={javaMode() === "auto"}>
            <SettingsRow
              title={t("instanceDetail.settings.java.auto")}
              hint={t("instanceDetail.settings.java.autoHint", {
                need: requiredJava(),
              })}
            >
              <span class="is-readonly-value">
                {t("instanceDetail.settings.java.autoValue", {
                  version: requiredJava(),
                })}
              </span>
            </SettingsRow>
          </Show>
          <Show when={javaMode() === "version"}>
            <SettingsRow
              title={t("instanceDetail.settings.java.versionLabel")}
              hint={t("instanceDetail.settings.java.versionHint", {
                version: requiredJava(),
              })}
            >
              <NumberField
                value={Number(draft().custom_java_version) || requiredJava()}
                min={8}
                max={64}
                step={1}
                ariaLabel={t("instanceDetail.settings.java.versionLabel")}
                onChange={(value) =>
                  commitGroup(JAVA_FIELDS, {
                    java_type: "version",
                    custom_java_version: String(value),
                  })
                }
              />
            </SettingsRow>
          </Show>
          <Show when={javaMode() === "detected"}>
            <SettingsRow
              title={t("instanceDetail.settings.java.detectedLabel")}
              hint={
                javas().length > 0
                  ? t("instanceDetail.settings.java.detectedHint")
                  : t("instanceDetail.settings.java.noDetected")
              }
            >
              <RmSelect
                value={selectedDetectedJava()?.path ?? ""}
                disabled={javas().length === 0}
                onChange={chooseDetectedJava}
                options={[
                  {
                    value: "",
                    label: t("instanceDetail.settings.java.chooseDetected"),
                  },
                  ...javas().map((java) => ({
                    value: java.path,
                    label: javaRuntimeLabel(java),
                  })),
                ]}
              />
            </SettingsRow>
          </Show>
          <Show when={javaMode() === "custom"}>
            <SettingsRow
              stacked
              title={t("instanceDetail.settings.java.customLabel")}
              hint={t("instanceDetail.settings.java.customHint")}
            >
              <div class="is-path-field">
                <RmInput
                  mono
                  value={draft().custom_java_path}
                  placeholder={t("instanceDetail.settings.java.customPlaceholder")}
                  onInput={(value) =>
                    commitGroup(
                      JAVA_FIELDS,
                      { java_type: "custom", custom_java_path: value }
                    )
                  }
                />
                <RmButton size="sm" icon="folder" onClick={() => void pickJavaDirectory()}>
                  {t("instanceDetail.settings.browse")}
                </RmButton>
              </div>
            </SettingsRow>
          </Show>
          <Show when={javaWarning()}>
            {(warning) => (
              <div class="is-inline-warning">
                <Icon name="alert" size={14} />
                <span>{warning()}</span>
              </div>
            )}
          </Show>
        </SettingsCard>

        <SettingsCard
          title={t("instanceDetail.settings.card.memory")}
          hint={t("instanceDetail.settings.card.memoryHint")}
          group={groupCard("memory")}
        >
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.memory.max")}
            hint={
              <>
                {t("instanceDetail.settings.memory.suggested", {
                  value: formatMemoryMb(limits().suggestedMb),
                })}
                <Show when={limits().totalMb === 0}>
                  <span> · {t("instanceDetail.settings.memory.unknown")}</span>
                </Show>
              </>
            }
          >
            <div class="is-memory-editor">
              <RmSlider
                value={draft().max_memory_mb}
                min={512}
                max={memoryMax()}
                step={512}
                label={t("instanceDetail.settings.memory.max")}
                onInput={setMemory}
              />
              <NumberField
                value={draft().max_memory_mb}
                min={512}
                max={memoryMax()}
                step={512}
                suffix="MiB"
                ariaLabel={t("instanceDetail.settings.memory.max")}
                onChange={setMemory}
              />
              <button
                type="button"
                class="is-suggest"
                onClick={() => setMemory(limits().suggestedMb)}
              >
                {t("instanceDetail.settings.memory.useSuggested", {
                  value: formatMemoryMb(limits().suggestedMb),
                })}
              </button>
            </div>
            <MemoryMeter
              totalBytes={memoryInfo()?.total_bytes ?? 0}
              availableBytes={memoryInfo()?.available_bytes ?? 0}
              allocatedMb={draft().max_memory_mb}
            />
            <Show when={memoryPercent() > 80}>
              <div class="is-inline-warning">
                <Icon name="alert" size={14} />
                <span>{t("instanceDetail.settings.memory.tooHigh")}</span>
              </div>
            </Show>
          </SettingsRow>
          <SettingsRow
            title={t("instanceDetail.settings.memory.min")}
            hint={t("instanceDetail.settings.memory.minHint")}
          >
            <NumberField
              value={draft().min_memory_mb ?? 0}
              min={0}
              max={memoryMax()}
              step={256}
              suffix="MiB"
              ariaLabel={t("instanceDetail.settings.memory.min")}
              onChange={(value) => commitField("min_memory_mb", value)}
            />
          </SettingsRow>
          <SettingsRow
            title={t("instanceDetail.settings.memory.metaspace")}
            hint={t("instanceDetail.settings.memory.metaspaceHint")}
          >
            <NumberField
              value={draft().metaspace_mb ?? 0}
              min={0}
              max={4096}
              step={64}
              suffix="MiB"
              ariaLabel={t("instanceDetail.settings.memory.metaspace")}
              onChange={(value) => commitField("metaspace_mb", value)}
            />
          </SettingsRow>
        </SettingsCard>

        <SettingsCard
          title={t("instanceDetail.settings.card.window")}
          hint={t("instanceDetail.settings.card.windowHint")}
          group={groupCard("window")}
        >
          <SettingsRow
            title={t("instanceDetail.settings.window.mode")}
            hint={t("instanceDetail.settings.window.modeHint")}
          >
            <RmSegmented
              value={draft().window_type === "fullscreen" ? "fullscreen" : "windowed"}
              label={t("instanceDetail.settings.window.mode")}
              options={[
                { value: "windowed", label: t("instanceDetail.settings.window.windowed") },
                { value: "fullscreen", label: t("instanceDetail.settings.window.fullscreen") },
              ]}
              onChange={(value) =>
                commitField("window_type", value, { immediate: true })
              }
            />
          </SettingsRow>
          <Show when={draft().window_type !== "fullscreen"}>
            <SettingsRow
              stacked
              title={t("instanceDetail.settings.window.resolution")}
              hint={t("instanceDetail.settings.window.resolutionHint")}
            >
              <div class="is-resolution">
                <RmSelect
                  class="is-resolution-preset"
                  value={resolutionValue()}
                  onChange={setResolutionPreset}
                  options={[
                    ...RESOLUTION_PRESETS.map((preset) => ({
                      value: resolutionKey(preset.width, preset.height),
                      label: preset.label,
                    })),
                    ...(resolution()
                      ? []
                      : [
                          {
                            value: "custom",
                            label: t("instanceDetail.settings.window.custom"),
                          },
                        ]),
                  ]}
                />
                <NumberField
                  value={draft().width}
                  min={320}
                  max={7680}
                  step={1}
                  suffix="px"
                  ariaLabel={t("instanceDetail.settings.window.width")}
                  onChange={(value) => commitField("width", value)}
                />
                <span class="is-times">×</span>
                <NumberField
                  value={draft().height}
                  min={240}
                  max={4320}
                  step={1}
                  suffix="px"
                  ariaLabel={t("instanceDetail.settings.window.height")}
                  onChange={(value) => commitField("height", value)}
                />
              </div>
            </SettingsRow>
          </Show>
          <Show when={draft().window_type === "fullscreen"}>
            <div class="is-row-note">{t("instanceDetail.settings.window.fullscreenHint")}</div>
          </Show>
        </SettingsCard>

        <SettingsCard
          title={t("instanceDetail.settings.card.quickPlay")}
          hint={t("instanceDetail.settings.card.quickPlayHint")}
          group={groupCard("quickPlay")}
        >
          <SettingsRow
            title={t("instanceDetail.settings.quickPlay.mode")}
            hint={t("instanceDetail.settings.quickPlay.modeHint")}
          >
            <RmSegmented
              value={draft().quick_play}
              label={t("instanceDetail.settings.quickPlay.mode")}
              options={[
                { value: "none", label: t("instanceDetail.settings.quickPlay.none") },
                {
                  value: "singleplayer",
                  label: t("instanceDetail.settings.quickPlay.singleplayer"),
                },
                {
                  value: "multiplayer",
                  label: t("instanceDetail.settings.quickPlay.multiplayer"),
                },
                { value: "realms", label: t("instanceDetail.settings.quickPlay.realms") },
              ]}
              onChange={setQuickPlay}
            />
          </SettingsRow>
          <Show when={quickPlayTarget()}>
            {(target) => (
              <SettingsRow
                stacked
                title={target().label}
                hint={t("instanceDetail.settings.quickPlay.targetHint")}
              >
                <RmInput
                  value={
                    String(
                      (draft() as unknown as Record<string, unknown>)[target().field] ?? ""
                    )
                  }
                  placeholder={target().placeholder}
                  onInput={(value) => setQuickTarget(target().field, value)}
                />
              </SettingsRow>
            )}
          </Show>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection
        icon="play"
        title={t("instanceDetail.settings.section.launch")}
        hint={t("instanceDetail.settings.section.launchHint")}
      >
        <SettingsCard
          title={t("instanceDetail.settings.card.process")}
          hint={t("instanceDetail.settings.card.processHint")}
          group={groupCard("process")}
        >
          <SettingsRow
            title={t("instanceDetail.settings.priority")}
            hint={t("instanceDetail.settings.priorityHint")}
          >
            <RmSelect
              value={draft().process_priority}
              onChange={(value) =>
                commitField("process_priority", value, { immediate: true })
              }
              options={[
                { value: "low", label: t("instanceDetail.settings.priority.low") },
                {
                  value: "below_normal",
                  label: t("instanceDetail.settings.priority.below_normal"),
                },
                { value: "normal", label: t("instanceDetail.settings.priority.normal") },
                {
                  value: "above_normal",
                  label: t("instanceDetail.settings.priority.above_normal"),
                },
                { value: "high", label: t("instanceDetail.settings.priority.high") },
              ]}
            />
          </SettingsRow>
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.runningDirectory")}
            hint={t("instanceDetail.settings.runningDirectoryHint")}
          >
            <div class="is-path-field">
              <RmInput
                mono
                value={draft().running_directory ?? ""}
                placeholder={paths().run_dir}
                onInput={(value) => commitField("running_directory", value)}
              />
              <RmButton
                size="sm"
                icon="folder"
                onClick={() => {
                  void dialog.pickDirectory().then((res) => {
                    if (!res.ok) {
                      toast({ message: res.error.message, tone: "error" });
                      return;
                    }
                    if (res.data) {
                      commitField("running_directory", res.data, { immediate: true });
                    }
                  });
                }}
              >
                {t("instanceDetail.settings.browse")}
              </RmButton>
              <Show when={draft().running_directory}>
                <RmButton
                  size="sm"
                  variant="ghost"
                  onClick={() => commitField("running_directory", "", { immediate: true })}
                >
                  {t("instanceDetail.settings.clear")}
                </RmButton>
              </Show>
            </div>
          </SettingsRow>
        </SettingsCard>

        <SettingsCard
          title={t("instanceDetail.settings.card.commands")}
          hint={t("instanceDetail.settings.card.commandsHint")}
          group={groupCard("commands")}
        >
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.wrapper")}
            hint={t("instanceDetail.settings.wrapperHint")}
          >
            <RmInput
              mono
              value={draft().wrapper}
              placeholder="flatpak run …"
              onInput={(value) => commitField("wrapper", value)}
            />
          </SettingsRow>
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.preCommand")}
            hint={t("instanceDetail.settings.preCommandHint")}
          >
            <RmInput
              mono
              value={draft().pre_launch_command}
              onInput={(value) => commitField("pre_launch_command", value)}
            />
          </SettingsRow>
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.postCommand")}
            hint={t("instanceDetail.settings.postCommandHint")}
          >
            <RmInput
              mono
              value={draft().post_exit_command}
              onInput={(value) => commitField("post_exit_command", value)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <section class={`is-section is-advanced${advancedOpen() ? " is-open" : ""}`}>
        <button
          type="button"
          class="is-section-head is-section-toggle"
          aria-expanded={advancedOpen()}
          onClick={() => setAdvancedOpen(!advancedOpen())}
        >
          <span class="is-section-icon" aria-hidden="true">
            <Icon name="sliders" size={15} />
          </span>
          <div class="is-section-heading">
            <h3 class="is-section-title">{t("instanceDetail.settings.section.advanced")}</h3>
            <p class="is-section-hint">
              {t("instanceDetail.settings.section.advancedHint")}
            </p>
          </div>
          <Icon name={advancedOpen() ? "chevron-up" : "chevron-down"} size={16} />
        </button>
        <Show when={advancedOpen()}>
          <div class="is-section-body">
            <AdvancedCards
              draft={draft}
              groupCard={groupCard}
              jvmText={jvmText}
              setJvmText={(value) => {
                setJvmText(value);
                setArgsText(JVM_FIELDS, "jvm_args", value);
              }}
              gameArgsText={gameArgsText}
              setGameArgsText={(value) => {
                setGameArgsText(value);
                setArgsText(GAME_FIELDS, "game_args", value);
              }}
              agentsText={agentsText}
              setAgentsText={(value) => {
                setAgentsText(value);
                setArgsText(AGENT_FIELDS, "java_agents", value);
              }}
              envText={envText}
              setEnv={setEnv}
              envInvalid={envInvalid()}
              commitField={commitField}
            />
          </div>
        </Show>
      </section>

      <SettingsSection
        icon="shield"
        title={t("instanceDetail.settings.section.maintenance")}
        hint={t("instanceDetail.settings.section.maintenanceHint")}
      >
        <SettingsCard
          title={t("instanceDetail.maintenance.isolation")}
          hint={t("instanceDetail.maintenance.isolationWarning")}
          actions={
            <Show when={launchStore.isRunning(key())}>
              <RmTag tone="warning">{t("instances.running")}</RmTag>
            </Show>
          }
        >
          <SettingsRow
            title={t("instanceDetail.settings.isolationPolicy")}
            hint={t("instanceDetail.settings.isolationPolicyHint")}
          >
            <RmSelect
              value={isolation()}
              disabled={launchStore.isRunning(key())}
              onChange={(value) => void changeIsolation(value)}
              options={[
                {
                  value: "",
                  label: t("instanceDetail.settings.isolationInherit", {
                    value: isolationLabel(props.detail.effective.isolation),
                  }),
                },
                {
                  value: "never",
                  label: t("instanceDetail.maintenance.isolationShared"),
                },
                {
                  value: "modded",
                  label: t("instanceDetail.maintenance.isolationModded"),
                },
                {
                  value: "always",
                  label: t("instanceDetail.maintenance.isolationIsolated"),
                },
              ]}
            />
          </SettingsRow>
          <SettingsRow
            stacked
            title={t("instanceDetail.settings.runningDirectoryResolved")}
            hint={t("instanceDetail.settings.runningDirectoryResolvedHint")}
          >
            <div class="is-path-field">
              <span class="is-path-value" title={paths().run_dir}>
                {paths().run_dir}
              </span>
              <RmButton size="sm" icon="folder" onClick={() => void reveal("run")}>
                {t("instanceDetail.settings.openFolder")}
              </RmButton>
            </div>
          </SettingsRow>
        </SettingsCard>

        <SettingsCard
          title={t("instanceDetail.maintenance.completion")}
          hint={t("instanceDetail.settings.completionHint")}
          actions={
            <div class="btn-row">
              <RmButton size="sm" icon="shield" loading={checking()} onClick={() => void checkCompletion()}>
                {t("instanceDetail.maintenance.check")}
              </RmButton>
              <Show when={(missing()?.length ?? 0) > 0}>
                <RmButton
                  size="sm"
                  variant="primary"
                  icon="wrench"
                  loading={repairing()}
                  onClick={() => void repairInstance()}
                >
                  {t("instanceDetail.maintenance.repair", { n: missing()!.length })}
                </RmButton>
              </Show>
            </div>
          }
        >
          <Show
            when={missing()}
            fallback={
              <div class="is-empty-line">
                {t("instanceDetail.settings.completionNotChecked")}
              </div>
            }
          >
            {(list) => (
              <Show
                when={list().length > 0}
                fallback={
                  <div class="is-ok-line">
                    <Icon name="check" size={14} />
                    {t("instanceDetail.settings.completionClean")}
                  </div>
                }
              >
                <div class="rm-list">
                  <For each={list()}>
                    {(file) => (
                      <RmListRow
                        icon={<Icon name="alert" size={15} class="is-alert-icon" />}
                        title={file.path}
                        subtitle={`${file.kind} · ${file.description}`}
                      />
                    )}
                  </For>
                </div>
              </Show>
            )}
          </Show>
        </SettingsCard>
      </SettingsSection>

      <div class="is-save-status" aria-live="polite">
        <span
          class={`is-save-dot${saving() ? " is-saving" : ""}${saveError() ? " is-error" : ""}`}
        />
        <Show
          when={saving()}
          fallback={
            <Show
              when={saveError()}
              fallback={
                <span>
                  {savedAt()
                    ? t("instanceDetail.settings.saved")
                    : t("instanceDetail.settings.autoSave")}
                </span>
              }
            >
              <span class="is-save-error">
                {t("instanceDetail.settings.saveFailed", { message: saveError() ?? "" })}
              </span>
            </Show>
          }
        >
          <span>{t("instanceDetail.settings.saving")}</span>
        </Show>
      </div>
    </div>
  );
}

// ── Advanced cards ────────────────────────────────────────────────────────

function AdvancedCards(props: {
  draft: () => ResolvedInstanceSettings;
  groupCard: (group: SettingGroupId) => {
    overridden: boolean;
    onToggle: () => void;
    label: string;
  };
  jvmText: () => string;
  setJvmText: (value: string) => void;
  gameArgsText: () => string;
  setGameArgsText: (value: string) => void;
  agentsText: () => string;
  setAgentsText: (value: string) => void;
  envText: () => string;
  setEnv: (value: string) => void;
  envInvalid: number;
  commitField: (field: string, value: unknown, opts?: { immediate?: boolean }) => void;
}): JSX.Element {
  return (
    <>
      <SettingsCard
        title={t("instanceDetail.settings.card.jvm")}
        hint={t("instanceDetail.settings.card.jvmHint")}
        group={props.groupCard("jvm")}
      >
        <SettingsRow
          stacked
          title={t("instanceDetail.settings.jvmArgs")}
          hint={t("instanceDetail.settings.jvmArgsHint")}
        >
          <textarea
            class="rm-input rm-textarea is-code"
            rows={4}
            value={props.jvmText()}
            placeholder="-XX:+UseG1GC"
            onInput={(e) => props.setJvmText(e.currentTarget.value)}
          />
        </SettingsRow>
        <SettingsRow
          title={t("instanceDetail.settings.field.no_generated_jvm_args")}
          hint={t("instanceDetail.settings.noGeneratedHint")}
        >
          <RmSwitch
            checked={props.draft().no_generated_jvm_args}
            label={t("instanceDetail.settings.field.no_generated_jvm_args")}
            onChange={(value) =>
              props.commitField("no_generated_jvm_args", value, { immediate: true })
            }
          />
        </SettingsRow>
        <SettingsRow
          title={t("instanceDetail.settings.field.no_generated_optimizing_jvm_args")}
          hint={t("instanceDetail.settings.noOptimizingHint")}
        >
          <RmSwitch
            checked={props.draft().no_generated_optimizing_jvm_args}
            disabled={props.draft().no_generated_jvm_args}
            label={t("instanceDetail.settings.field.no_generated_optimizing_jvm_args")}
            onChange={(value) =>
              props.commitField("no_generated_optimizing_jvm_args", value, {
                immediate: true,
              })
            }
          />
        </SettingsRow>
        <SettingsRow
          title={t("instanceDetail.settings.notCheckJvm")}
          hint={t("instanceDetail.settings.notCheckJvmHint")}
        >
          <RmSwitch
            checked={props.draft().not_check_jvm}
            label={t("instanceDetail.settings.notCheckJvm")}
            onChange={(value) => props.commitField("not_check_jvm", value, { immediate: true })}
          />
        </SettingsRow>
      </SettingsCard>

      <SettingsCard
        title={t("instanceDetail.settings.card.game")}
        hint={t("instanceDetail.settings.card.gameHint")}
        group={props.groupCard("game")}
      >
        <SettingsRow
          stacked
          title={t("instanceDetail.settings.gameArgs")}
          hint={t("instanceDetail.settings.gameArgsHint")}
        >
          <textarea
            class="rm-input rm-textarea is-code"
            rows={3}
            value={props.gameArgsText()}
            placeholder="--demo"
            onInput={(e) => props.setGameArgsText(e.currentTarget.value)}
          />
        </SettingsRow>
        <SettingsRow
          stacked
          title={t("instanceDetail.settings.envVars")}
          hint={
            <>
              {t("instanceDetail.settings.envVarsHint")}
              <Show when={props.envInvalid > 0}>
                <span class="is-inline-warning-text">
                  {" "}
                  {t("instanceDetail.settings.envInvalid", { n: props.envInvalid })}
                </span>
              </Show>
            </>
          }
        >
          <textarea
            class="rm-input rm-textarea is-code"
            rows={3}
            value={props.envText()}
            placeholder="JAVA_HOME=/path/to/jdk"
            onInput={(e) => props.setEnv(e.currentTarget.value)}
          />
        </SettingsRow>
      </SettingsCard>

      <SettingsCard
        title={t("instanceDetail.settings.card.agents")}
        hint={t("instanceDetail.settings.card.agentsHint")}
        group={props.groupCard("agents")}
      >
        <SettingsRow
          stacked
          title={t("instanceDetail.settings.agents")}
          hint={t("instanceDetail.settings.agentsHint")}
        >
          <textarea
            class="rm-input rm-textarea is-code"
            rows={3}
            value={props.agentsText()}
            placeholder="/path/to/agent.jar"
            onInput={(e) => props.setAgentsText(e.currentTarget.value)}
          />
        </SettingsRow>
      </SettingsCard>

      <SettingsCard
        title={t("instanceDetail.settings.card.natives")}
        hint={t("instanceDetail.settings.card.nativesHint")}
        group={props.groupCard("natives")}
      >
        <SettingsRow
          title={t("instanceDetail.settings.field.use_native_glfw")}
          hint={t("instanceDetail.settings.nativeGlfwHint")}
        >
          <RmSwitch
            checked={props.draft().use_native_glfw}
            label={t("instanceDetail.settings.field.use_native_glfw")}
            onChange={(value) => props.commitField("use_native_glfw", value, { immediate: true })}
          />
        </SettingsRow>
        <SettingsRow
          title={t("instanceDetail.settings.field.use_native_openal")}
          hint={t("instanceDetail.settings.nativeOpenalHint")}
        >
          <RmSwitch
            checked={props.draft().use_native_openal}
            label={t("instanceDetail.settings.field.use_native_openal")}
            onChange={(value) =>
              props.commitField("use_native_openal", value, { immediate: true })
            }
          />
        </SettingsRow>
        <SettingsRow
          title={t("instanceDetail.settings.customNatives")}
          hint={t("instanceDetail.settings.customNativesHint")}
        >
          <RmSwitch
            checked={props.draft().use_custom_natives}
            label={t("instanceDetail.settings.customNatives")}
            onChange={(value) =>
              props.commitField("use_custom_natives", value, { immediate: true })
            }
          />
        </SettingsRow>
      </SettingsCard>

      <SettingsCard
        title={t("instanceDetail.settings.card.debug")}
        hint={t("instanceDetail.settings.card.debugHint")}
        group={props.groupCard("debug")}
      >
        <SettingsRow
          title={t("instanceDetail.settings.field.enable_debug_log_output")}
          hint={t("instanceDetail.settings.debugLogHint")}
        >
          <RmSwitch
            checked={props.draft().enable_debug_log_output}
            label={t("instanceDetail.settings.field.enable_debug_log_output")}
            onChange={(value) =>
              props.commitField("enable_debug_log_output", value, { immediate: true })
            }
          />
        </SettingsRow>
      </SettingsCard>
    </>
  );
}
