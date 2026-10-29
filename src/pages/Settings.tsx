/**
 * Settings (ui-design 08): seven hash sections (game defaults / general /
 * download / AI & control / plugins host / advanced / about).
 * The #ai section implements ui-design 07 §2 (AI service + MCP management).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, Switch, Match } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmSwitch, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmCard, RmSideTabs, RmListRow, RmAdvancedSection, RmFieldRow, RmEmptyState, RmErrorState } from "../components/rm/surfaces";
import { settings as settingsApi, userSettings, instances, gameDirectories, logs, external, mcp as mcpApi, ai as aiApi, plugins as pluginsApi, dashboard, storage } from "../ipc/api";
import type { AppInfoView, JavaInfo, InstanceDetail, LocaleInfo, McpToolView, ThemeTokens, GlobalGameSettings, GameDirectory, StorageOverview } from "../ipc/types";
import { Icon } from "../components/icons";
import { settingsStore } from "../stores/settings";
import { aiStore } from "../stores/ai";
import { mcpStore } from "../stores/mcp";
import { pluginsStore } from "../stores/plugins";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { listThemes, visualPaletteFor } from "../theme";
import {
  adaptiveEnabled,
  adaptivePalette,
  adaptiveSource,
  adaptiveStatus,
  disableAdaptive,
  enableAdaptive,
  refreshAdaptivePalette,
} from "../lib/adaptiveTheme";
import { cacheStore } from "../stores/cache";
import { formatBytes } from "../lib/format";
import { t } from "../i18n";
import { loaderLabel } from "../lib/loaders";
import type { TranslationKey } from "../i18n/locales/messages";
import { elidePath } from "../lib/format";
import { useHashTab } from "../lib/useHashTab";
import { homeCardOpacity, panelOpacity, setHomeCardOpacity, setPanelOpacity } from "../lib/glass";
import {
  backgroundVideo,
  backgroundVideoIssue,
  clearBackgroundVideo,
  pickBackgroundVideo,
  setBackgroundVideoDim,
  setBackgroundVideoEnabled,
} from "../lib/backgroundVideo";

const SECTIONS: { id: string; icon: "list" | "settings" | "download" | "bot" | "extension" | "wrench" | "info" | "sparkles" | "hard-drive"; label: TranslationKey }[] = [
  { id: "game-defaults", icon: "list", label: "settings.nav.gameDefaults" },
  { id: "general", icon: "settings" as const, label: "settings.nav.general" },
  { id: "appearance", icon: "sparkles" as const, label: "settings.nav.appearance" },
  { id: "download", icon: "download" as const, label: "settings.nav.download" },
  { id: "storage", icon: "hard-drive" as const, label: "settings.nav.storage" },
  { id: "ai", icon: "bot" as const, label: "settings.nav.ai" },
  { id: "plugins", icon: "extension" as const, label: "settings.nav.plugins" },
  { id: "advanced", icon: "wrench" as const, label: "settings.nav.advanced" },
  { id: "about", icon: "info" as const, label: "settings.nav.about" },
];

export default function Settings(): JSX.Element {
  const [section, setSection] = useHashTab("game-defaults", SECTIONS.map((s) => s.id));

  onMount(() => {
    void settingsStore.refresh();
  });

  return (
    <div class="page">
      <PageHeader title={t("settings.title")} />
      <div class="page-body page-with-sidetabs">
        <RmSideTabs
          items={SECTIONS.map((s) => ({ ...s, label: t(s.label) }))}
          active={section()}
          onSelect={setSection}
        />
        <div class="side-tab-content settings-content">
          <Switch>
            <Match when={section() === "game-defaults"}><GameDefaults /></Match>
            <Match when={section() === "general"}><General /></Match>
            <Match when={section() === "appearance"}><Appearance /></Match>
            <Match when={section() === "download"}><DownloadSection /></Match>
            <Match when={section() === "storage"}><StorageSection /></Match>
            <Match when={section() === "ai"}><AiSection /></Match>
            <Match when={section() === "plugins"}><PluginsHost /></Match>
            <Match when={section() === "advanced"}><Advanced /></Match>
            <Match when={section() === "about"}><About /></Match>
          </Switch>
        </div>
      </div>
    </div>
  );
}

// ── Game defaults ──

function GameDefaults(): JSX.Element {
  const navigate = useNavigate();
  const config = () => settingsStore.config();
  const [rootDir, setRootDir] = createSignal("");
  const [rootDirty, setRootDirty] = createSignal(false);
  const [javas, setJavas] = createSignal<JavaInfo[]>([]);
  const [autoMemory, setAutoMemory] = createSignal(false);
  const [gameSettings, setGameSettings] = createSignal<GlobalGameSettings | null>(null);
  const [selectedDirectory, setSelectedDirectory] = createSignal<GameDirectory | null>(null);

  // Explicit-save drafts (P1-12): edit locally, one IPC per Save click. The
  // `touched` flags stop the config backfill below from clobbering in-progress
  // edits when another section's save returns a fresh config (P1-1).
  const [minMem, setMinMem] = createSignal(1024);
  const [maxMem, setMaxMem] = createSignal(4096);
  const [winW, setWinW] = createSignal(854);
  const [winH, setWinH] = createSignal(480);
  const [jvmText, setJvmText] = createSignal("");
  const [gameText, setGameText] = createSignal("");
  const touched = { memory: false, window: false, jvm: false, game: false };

  createEffect(() => {
    const c = config();
    if (!c) return;
    if (!rootDirty()) setRootDir(c.game_root_dir ?? "");
    setAutoMemory(c.auto_memory);
    if (!touched.memory) {
      setMinMem(c.default_min_memory_mb ?? 1024);
      setMaxMem(c.default_max_memory_mb ?? 4096);
    }
    if (!touched.window) {
      setWinW(c.default_window_width ?? 854);
      setWinH(c.default_window_height ?? 480);
    }
    if (!touched.jvm) setJvmText((c.default_jvm_args ?? []).join("\n"));
    if (!touched.game) setGameText((c.default_game_args ?? []).join("\n"));
  });

  createEffect(() => {
    const gs = gameSettings();
    if (!gs) return;
    if (!touched.memory) {
      setMinMem(gs.min_memory_mb ?? 1024);
      setMaxMem(gs.max_memory_mb);
    }
    if (!touched.window) {
      setWinW(gs.width);
      setWinH(gs.height);
    }
    if (!touched.jvm) setJvmText(gs.jvm_args.join("\n"));
    if (!touched.game) setGameText(gs.game_args.join("\n"));
    setAutoMemory(gs.auto_memory);
  });

  onMount(() => {
    void settingsApi.detectJava().then((r) => r.ok && setJavas(r.data));
    void instances.globalSettings().then((r) => r.ok && setGameSettings(r.data));
    void gameDirectories.selected().then((r) => r.ok && setSelectedDirectory(r.data));
  });

  async function patch(update: Parameters<typeof settingsStore.update>[0]): Promise<boolean> {
    const res = await settingsStore.update(update);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
    return res.ok;
  }

  async function patchGame(update: Partial<GlobalGameSettings>): Promise<boolean> {
    const current = gameSettings();
    if (!current) return false;
    const res = await instances.updateGlobalSettings({ ...current, ...update });
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return false;
    }
    setGameSettings(res.data);
    return true;
  }

  return (
    <div class="tab-stack">
      <Show
        when={config()}
        fallback={
          <Show
            when={settingsStore.loadError()}
            fallback={<div class="muted-line">{t("common.loading")}</div>}
          >
            <RmErrorState message={settingsStore.loadError()!} />
          </Show>
        }
      >
      <div class="info-note">{t("settings.game.intro")}</div>
      <RmCard title={t("settings.game.rootDir")}>
        <div class="btn-row">
          <span class="rm-input" style={{ flex: "1", "min-width": "220px" }}>
            {selectedDirectory() ? elidePath(selectedDirectory()!.path, 52) : "-"}
          </span>
          <RmButton size="sm" icon="folder" onClick={() => navigate("/instances")}>
            {t("instances.title")}
          </RmButton>
        </div>
        <div class="muted-line">{t("settings.game.rootDirNote")}</div>
      </RmCard>

      <RmCard title={t("settings.game.java")}>
        <RmFieldRow label={t("settings.game.javaDefault")}>
          <RmSelect
            value={gameSettings()?.java_type === "custom" ? gameSettings()!.custom_java_path : ""}
            onChange={(v) =>
              void patchGame({
                java_type: v ? "custom" : "auto",
                custom_java_path: v,
              })
            }
            options={[
              { value: "", label: t("instanceDetail.settings.javaAuto") },
              ...javas().map((j) => ({ value: j.path, label: `Java ${j.major_version} · ${j.vendor}` })),
            ]}
          />
        </RmFieldRow>
      </RmCard>

      <RmCard title={t("settings.game.isolation")}>
        <RmFieldRow
          label={t("instanceDetail.maintenance.isolation")}
          hint={<span class="muted-line">{t("settings.game.isolationHint")}</span>}
        >
          <RmSelect
            value={gameSettings()?.default_isolation ?? "modded"}
            onChange={(v) => void patchGame({ default_isolation: v as GlobalGameSettings["default_isolation"] })}
            options={[
              { value: "never", label: t("instanceDetail.maintenance.isolationShared") },
              { value: "modded", label: t("instanceDetail.maintenance.isolationModded") },
              { value: "always", label: t("instanceDetail.maintenance.isolationIsolated") },
            ]}
          />
        </RmFieldRow>
      </RmCard>

      <RmCard title={t("settings.game.memory")}>
        <RmFieldRow label={t("settings.game.autoMemory")}>
          <RmSwitch
            checked={autoMemory()}
            onChange={(v) => {
              setAutoMemory(v);
              void patchGame({ auto_memory: v });
            }}
          />
        </RmFieldRow>
        <Show when={!autoMemory()}>
          <RmFieldRow label={t("settings.game.memoryRange")}>
            <div class="btn-row">
              <input
                class="rm-input"
                type="number"
                style={{ width: "110px" }}
                value={minMem()}
                onInput={(e) => {
                  touched.memory = true;
                  setMinMem(Number(e.currentTarget.value));
                }}
              />
              <span>–</span>
              <input
                class="rm-input"
                type="number"
                style={{ width: "110px" }}
                value={maxMem()}
                onInput={(e) => {
                  touched.memory = true;
                  setMaxMem(Number(e.currentTarget.value));
                }}
              />
              <span>MiB</span>
              <RmButton
                size="sm"
                variant="primary"
                onClick={() =>
                  void patchGame({ min_memory_mb: minMem(), max_memory_mb: maxMem() })
                    .then((ok) => ok && (touched.memory = false))
                }
              >
                {t("common.save")}
              </RmButton>
            </div>
          </RmFieldRow>
        </Show>
      </RmCard>

      <RmCard title={t("settings.game.window")}>
        <div class="btn-row">
          <input
            class="rm-input"
            type="number"
            style={{ width: "100px" }}
            value={winW()}
            onInput={(e) => {
              touched.window = true;
              setWinW(Number(e.currentTarget.value));
            }}
          />
          ×
          <input
            class="rm-input"
            type="number"
            style={{ width: "100px" }}
            value={winH()}
            onInput={(e) => {
              touched.window = true;
              setWinH(Number(e.currentTarget.value));
            }}
          />
          <RmButton
            size="sm"
            variant="primary"
            onClick={() =>
              void patchGame({ width: winW(), height: winH() })
                .then((ok) => ok && (touched.window = false))
            }
          >
            {t("common.save")}
          </RmButton>
          <RmSwitch
            checked={gameSettings()?.window_type === "fullscreen"}
            onChange={(v) => void patchGame({ window_type: v ? "fullscreen" : "windowed" })}
          />
          <span>{t("instanceDetail.settings.fullscreen")}</span>
        </div>
      </RmCard>

      <RmAdvancedSection title={t("settings.game.advancedArgs")} summary={t("instanceDetail.settings.argsSummary")}>
        <RmFieldRow label={t("settings.game.jvmArgs")}>
          <textarea
            class="rm-textarea rm-input-mono"
            rows={3}
            value={jvmText()}
            onInput={(e) => {
              touched.jvm = true;
              setJvmText(e.currentTarget.value);
            }}
          />
          <div class="btn-row">
            <RmButton
              size="sm"
              variant="primary"
              onClick={() =>
                void patchGame({ jvm_args: jvmText().split("\n").map((l) => l.trim()).filter(Boolean) })
                  .then((ok) => ok && (touched.jvm = false))
              }
            >
              {t("common.save")}
            </RmButton>
          </div>
        </RmFieldRow>
        <RmFieldRow label={t("settings.game.gameArgs")}>
          <textarea
            class="rm-textarea rm-input-mono"
            rows={2}
            value={gameText()}
            onInput={(e) => {
              touched.game = true;
              setGameText(e.currentTarget.value);
            }}
          />
          <div class="btn-row">
            <RmButton
              size="sm"
              variant="primary"
              onClick={() =>
                void patchGame({ game_args: gameText().split("\n").map((l) => l.trim()).filter(Boolean) })
                  .then((ok) => ok && (touched.game = false))
              }
            >
              {t("common.save")}
            </RmButton>
          </div>
        </RmFieldRow>
      </RmAdvancedSection>
      </Show>
    </div>
  );
}

// ── General ──

/** Theme key the backend JSON/tokens endpoints understand for the current preference. */
function resolvedThemeKey(preference: string | null): string {
  if (preference === "light") return "builtin:light";
  if (preference === "dark") return "builtin:dark";
  if (!preference || preference === "system") {
    // "Follow system" resolves in the renderer; the applied mode is on <html>.
    return document.documentElement.dataset.themeMode === "light" ? "builtin:light" : "builtin:dark";
  }
  return preference; // plugin theme key passes through
}

function General(): JSX.Element {
  const navigate = useNavigate();
  const [locales, setLocales] = createSignal<LocaleInfo[]>([]);
  const [logLines, setLogLines] = createSignal<string[]>([]);
  const [javas, setJavas] = createSignal<JavaInfo[]>([]);
  const config = () => settingsStore.config();

  onMount(async () => {
    const localeRes = await userSettings.supportedLocales();
    if (localeRes.ok) setLocales(localeRes.data);
    const logRes = await logs.get(200);
    if (logRes.ok) setLogLines(logRes.data);
    void settingsApi.detectJava().then((r) => r.ok && setJavas(r.data));
  });

  return (
    <div class="tab-stack">
      <RmCard title={t("settings.general.language")}>
        <RmSelect
          value={settingsStore.user()?.locale ?? ""}
          onChange={(v) => {
            void import("../i18n").then(({ setLocale }) => setLocale(v === "" ? null : v));
          }}
          options={[
            { value: "", label: t("settings.general.followSystem") },
            ...locales().map((l) => ({ value: l.code, label: l.native_name })),
          ]}
        />
      </RmCard>

      <RmCard title={t("settings.general.debug.title")}>
        <RmFieldRow
          label={t("settings.general.debug.toggle")}
          hint={<span class="muted-line">{t("settings.general.debug.hint")}</span>}
        >
          <RmSwitch checked={settingsStore.debugMode()} onChange={(v) => settingsStore.setDebugMode(v)} />
        </RmFieldRow>
        <Show when={settingsStore.debugMode()}>
          <RmAdvancedSection title={t("settings.general.debug.infoTitle")}>
            <div class="kv-grid">
              <span>{t("settings.about.version")}</span><span>{__APP_VERSION__}</span>
              <span>{t("settings.game.rootDir")}</span><span>{config()?.game_root_dir ?? "-"}</span>
              <span>{t("settings.download.mirror")}</span><span>{config()?.mirror_url || "-"}</span>
              <span>{t("settings.download.concurrency")}</span><span>{config()?.download_concurrency ?? "-"}</span>
              <span>{t("settings.debug.isolation")}</span><span>{config()?.default_isolation ?? "-"}</span>
              <span>{t("settings.debug.java")}</span>
              <span>
                <For each={javas()}>{(j) => <div class="rm-input-mono" style={{ "font-size": "11px" }}>{`Java ${j.major_version} · ${j.path}`}</div>}</For>
              </span>
            </div>
          </RmAdvancedSection>
        </Show>
      </RmCard>

      <RmCard title={t("settings.general.log.title")}>
        <div class="btn-row">
          <RmButton size="sm" icon="refresh" onClick={async () => {
            const res = await logs.get(200);
            if (res.ok) setLogLines(res.data);
          }}>
            {t("common.refresh")}
          </RmButton>
          <RmButton size="sm" icon="copy" onClick={() => { void navigator.clipboard.writeText(logLines().join("\n")); toast({ message: t("logs.copied"), tone: "success" }); }}>
            {t("common.copy")}
          </RmButton>
          <RmButton
            size="sm"
            variant="danger"
            icon="trash"
            onClick={async () => {
              const allowed = await confirmDanger({
                title: t("settings.general.log.clear.title"),
                body: t("settings.general.log.clear.body"),
                confirmLabel: t("common.delete"),
              });
              if (allowed) {
                await logs.clear();
                setLogLines([]);
              }
            }}
          >
            {t("settings.general.log.clear.title")}
          </RmButton>
        </div>
        <pre class="log-viewer">
          <For each={logLines()}>{(line) => <div class={`log-line log-${line.includes("ERROR") ? "error" : line.includes("WARN") ? "warn" : "info"}`}>{line}</div>}</For>
        </pre>
      </RmCard>

      <RmCard title={t("settings.general.updates")}>
        {/* "Check now" stays hidden until the updater endpoint is real
            (P2-17, release-blocked): a permanently-disabled button is worse
            than no button. */}
        <div class="btn-row">
          <RmSwitch checked={settingsStore.user()?.check_updates ?? false} onChange={(v) => void settingsStore.updateUser({ check_updates: v, locale: null, theme: null })} />
          <span>{t("settings.general.updates.autoCheck")}</span>
        </div>
      </RmCard>
    </div>
  );
}

// ── Appearance（从通用拆出，2026-10-25）：配色 + 动效 ──

function Appearance(): JSX.Element {
  const [themes, setThemes] = createSignal<Awaited<ReturnType<typeof listThemes>>>([]);
  const [themeJson, setThemeJson] = createSignal<string>("");

  onMount(async () => setThemes(await listThemes()));

  const adaptiveColors = () =>
    adaptivePalette()?.colors ?? ["#6f91ca", "#8aa583", "#e4ce83"];
  const adaptivePanel = () => adaptivePalette()?.vars["--adaptive-raised"] ?? "#f8f8f4";
  const adaptiveSourceLabel = () => {
    const source = adaptiveSource();
    return t(
      `settings.appearance.adaptive.source.${source ?? "default"}` as TranslationKey
    );
  };
  const adaptiveSubtitle = () => {
    if (!adaptiveEnabled()) return t("settings.appearance.adaptive.subtitle");
    if (adaptiveStatus() === "sampling") return t("settings.appearance.adaptive.sampling");
    return t("settings.appearance.adaptive.active");
  };
  const adaptiveDetailText = () => {
    if (adaptiveStatus() === "sampling") return t("settings.appearance.adaptive.sampling");
    if (adaptiveStatus() === "fallback") return t("settings.appearance.adaptive.fallback");
    const palette = adaptivePalette();
    if (!palette) return t("settings.appearance.adaptive.sampling");
    const base = t("settings.appearance.adaptive.sampled", {
      source: adaptiveSourceLabel(),
      n: palette.colors.length,
    });
    return `${base} · ${
      palette.readable
        ? t("settings.appearance.adaptive.contrastOk")
        : t("settings.appearance.adaptive.lowContrast")
    }`;
  };

  async function chooseTheme(key: string): Promise<void> {
    const res = await settingsStore.setTheme(key);
    if (!res.ok) {
      toast({ message: res.message ?? "", tone: "error" });
      return;
    }
    if (adaptiveEnabled()) await disableAdaptive(false);
  }

  return (
    <div class="tab-stack narrow">
      <RmCard title={t("settings.general.appearance")}>
        {/* 「跟随系统」已按需求移除：只保留显式选择的主题卡片 */}
        <div class="theme-grid">
          <button
            class={`theme-card is-adaptive${adaptiveEnabled() ? " is-active" : ""}`}
            onClick={() => void enableAdaptive()}
          >
            <div class="adaptive-swatch" aria-hidden="true">
              <span style={{ background: adaptiveColors()[0] }} />
              <span style={{ background: adaptiveColors()[1] }} />
              <span style={{ background: adaptiveColors()[2] }} />
              <span class="adaptive-swatch-light" style={{ background: adaptivePanel() }} />
            </div>
            <span class="adaptive-title">
              <Icon name="sparkles" size={13} />
              {t("settings.appearance.adaptive.title")}
            </span>
            <small class="adaptive-subtitle">{adaptiveSubtitle()}</small>
          </button>
          <For each={themes()}>
            {(info) => (
              <button
                class={`theme-card${
                  !adaptiveEnabled() && settingsStore.themePreference() === info.key ? " is-active" : ""
                }`}
                onClick={() => void chooseTheme(info.key)}
              >
                <ThemeSwatch key_={info.key} />
                <span>{info.name}</span>
              </button>
            )}
          </For>
        </div>
        <Show when={adaptiveEnabled()}>
          <div
            class={`adaptive-detail${
              adaptiveStatus() === "fallback" || adaptivePalette()?.readable === false
                ? " is-warning"
                : ""
            }`}
          >
            <span class="adaptive-dots" aria-hidden="true">
              <For each={adaptiveColors()}>
                {(color) => <i style={{ background: color }} />}
              </For>
            </span>
            <span class="adaptive-detail-text">{adaptiveDetailText()}</span>
            <span class="adaptive-detail-actions">
              <RmButton
                size="sm"
                icon="refresh"
                loading={adaptiveStatus() === "sampling"}
                onClick={() => void refreshAdaptivePalette()}
              >
                {t("settings.appearance.adaptive.resample")}
              </RmButton>
              <RmButton
                size="sm"
                variant="ghost"
                onClick={() => void disableAdaptive()}
              >
                {t("settings.appearance.adaptive.disable")}
              </RmButton>
            </span>
          </div>
        </Show>
        <RmAdvancedSection title={t("settings.general.themeJson")}>
          <RmButton
            size="sm"
            onClick={async () => {
              const res = await dashboard.getThemeJson(resolvedThemeKey(settingsStore.themePreference()));
              if (res.ok) setThemeJson(JSON.stringify(res.data, null, 2));
            }}
          >
            {t("common.view")}
          </RmButton>
          <Show when={themeJson()}>
            <pre class="logic-result">{themeJson()}</pre>
          </Show>
        </RmAdvancedSection>
      </RmCard>

      <RmCard title={t("settings.appearance.glass.title")}>
        <RmFieldRow
          label={t("settings.appearance.glass.opacity")}
          hint={<span class="muted-line">{t("settings.appearance.glass.hint")}</span>}
        >
          <div class="glass-opacity-control">
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              value={panelOpacity()}
              aria-label={t("settings.appearance.glass.opacity")}
              onInput={(event) => setPanelOpacity(Number(event.currentTarget.value))}
            />
            <output>{panelOpacity()}%</output>
          </div>
        </RmFieldRow>
      </RmCard>

      <RmCard title={t("settings.appearance.cards.title")}>
        <RmFieldRow
          label={t("settings.appearance.cards.opacity")}
          hint={<span class="muted-line">{t("settings.appearance.cards.hint")}</span>}
        >
          <div class="glass-opacity-control">
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              value={homeCardOpacity()}
              aria-label={t("settings.appearance.cards.opacity")}
              onInput={(event) => setHomeCardOpacity(Number(event.currentTarget.value))}
            />
            <output>{homeCardOpacity()}%</output>
          </div>
        </RmFieldRow>
      </RmCard>

      <RmCard title={t("settings.appearance.video.title")}>
        <RmFieldRow
          label={t("settings.appearance.video.toggle")}
          hint={<span class="muted-line">{t("settings.appearance.video.hint")}</span>}
        >
          <RmSwitch
            checked={backgroundVideo().enabled}
            disabled={!backgroundVideo().path}
            onChange={(value) => setBackgroundVideoEnabled(value)}
          />
        </RmFieldRow>
        <RmFieldRow label={t("settings.appearance.video.file")}>
          <div class="video-background-control">
            <span
              class={`video-background-path${backgroundVideo().path ? "" : " is-empty"}`}
              title={backgroundVideo().path || undefined}
            >
              {backgroundVideo().path
                ? elidePath(backgroundVideo().path, 44)
                : t("settings.appearance.video.empty")}
            </span>
            <RmButton
              size="sm"
              icon="folder"
              onClick={() => {
                void pickBackgroundVideo().then((res) => {
                  if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
                });
              }}
            >
              {backgroundVideo().path
                ? t("settings.appearance.video.replace")
                : t("settings.appearance.video.pick")}
            </RmButton>
            <Show when={backgroundVideo().path}>
              <RmIconButton
                icon="trash"
                label={t("settings.appearance.video.clear")}
                danger
                onClick={() => clearBackgroundVideo()}
              />
            </Show>
          </div>
        </RmFieldRow>
        <Show when={backgroundVideo().path}>
          <RmFieldRow
            label={t("settings.appearance.video.dim")}
            hint={<span class="muted-line">{t("settings.appearance.video.dimHint")}</span>}
          >
            <div class="glass-opacity-control">
              <input
                type="range"
                min="0"
                max="80"
                step="1"
                value={backgroundVideo().dim}
                aria-label={t("settings.appearance.video.dim")}
                onInput={(event) => setBackgroundVideoDim(Number(event.currentTarget.value))}
              />
              <output>{backgroundVideo().dim}%</output>
            </div>
          </RmFieldRow>
        </Show>
        <Show when={backgroundVideoIssue()}>
          <p class="video-background-note is-error">
            {backgroundVideoIssue() === "decode"
              ? t("settings.appearance.video.issue")
              : backgroundVideoIssue()}
          </p>
        </Show>
        <Show when={!settingsStore.motionEnabled()}>
          <p class="video-background-note">{t("settings.appearance.video.motionOff")}</p>
        </Show>
      </RmCard>

      <RmCard title={t("settings.general.motion")}>
        <RmFieldRow
          label={t("settings.general.motion.toggle")}
          hint={<span class="muted-line">{t("settings.general.motion.hint")}</span>}
        >
          <RmSwitch
            checked={settingsStore.motionEnabled()}
            onChange={(v) => settingsStore.setMotionEnabled(v)}
          />
        </RmFieldRow>
      </RmCard>
    </div>
  );
}

/** Miniature three-color scene preview; decorative palette is host-derived. */
function ThemeSwatch(props: { key_: string }): JSX.Element {
  const [tokens, setTokens] = createSignal<ThemeTokens | null>(null);
  createEffect(() => {
    void import("../ipc/api").then(({ dashboard }) =>
      dashboard.getThemeTokens(props.key_).then((res) => {
        if (res.ok) setTokens(res.data);
      })
    );
  });
  const palette = () => {
    const value = tokens();
    return value ? visualPaletteFor(props.key_, value) : null;
  };
  return (
    <div class="theme-swatch">
      <Show when={tokens() && palette()}>
        <div
          class="theme-swatch-scene"
          style={{
            background: tokens()!.background,
            "border-color": tokens()!.border,
          }}
        >
          <i class="theme-swatch-orb is-a" style={{ background: palette()!.primary }} />
          <i class="theme-swatch-orb is-b" style={{ background: palette()!.secondary }} />
          <i class="theme-swatch-orb is-c" style={{ background: palette()!.tertiary }} />
          <i class="theme-swatch-cube" style={{ background: palette()!.primary }} />
          <span class="theme-swatch-panel" style={{ background: tokens()!.surfaceElevated }} />
        </div>
        <div class="theme-swatch-colors">
          <span style={{ background: palette()!.primary }} />
          <span style={{ background: palette()!.secondary }} />
          <span style={{ background: palette()!.tertiary }} />
        </div>
      </Show>
    </div>
  );
}

// ── Download settings ──

function DownloadSection(): JSX.Element {
  const config = () => settingsStore.config();
  const [conc, setConc] = createSignal(16);
  let concTouched = false;
  createEffect(() => {
    const c = config();
    if (c && !concTouched) setConc(c.download_concurrency ?? 16);
  });
  return (
    <div class="tab-stack">
      <RmCard title={t("settings.download.mirror")}>
        <For
          each={[
            { value: "auto", title: t("settings.download.mirror.auto"), hint: t("settings.download.mirror.autoHint") },
            { value: "", title: t("settings.download.mirror.official"), hint: t("settings.download.mirror.officialHint") },
            { value: "https://bmclapi2.bangbang93.com", title: t("settings.download.mirror.bmclapi"), hint: t("settings.download.mirror.bmclapiHint") },
          ]}
        >
          {(option) => (
            <RmListRow
              title={option.title}
              subtitle={option.hint}
              selected={(config()?.mirror_url ?? "") === option.value}
              onClick={() =>
                void settingsStore.update({ mirror_url: option.value }).then((res) => {
                  // `""` clears the mirror; `null` would be "field absent" on the wire.
                  if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
                })
              }
              trailing={
                <Show when={(config()?.mirror_url ?? "") === option.value}>
                  <RmTag tone="success">✓</RmTag>
                </Show>
              }
            />
          )}
        </For>
      </RmCard>
      <RmCard title={t("cache.title")}>
        <CacheManagerCard />
      </RmCard>
      <RmCard title={t("settings.download.concurrency")}>
        <div class="btn-row">
          <input
            class="rm-input"
            type="number"
            min={1}
            max={64}
            style={{ width: "110px" }}
            value={conc()}
            onInput={(e) => {
              concTouched = true;
              setConc(Number(e.currentTarget.value));
            }}
          />
          <span class="muted-line">{t("settings.download.concurrencyHint")}</span>
          <RmButton
            size="sm"
            variant="primary"
            onClick={() =>
              void settingsStore.update({ download_concurrency: conc() }).then((res) => {
                if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
                else concTouched = false;
              })
            }
          >
            {t("common.save")}
          </RmButton>
        </div>
      </RmCard>
    </div>
  );
}

// ── 图标缓存（从下载页缓存 Tab 迁入，2026-10-25）──

function CacheManagerCard(): JSX.Element {
  onMount(() => void cacheStore.refreshStats());
  const stats = () => cacheStore.stats();
  const [maxMiB, setMaxMiB] = createSignal(1024);
  createEffect(() => {
    if (stats()) setMaxMiB(Math.round(stats()!.maxBytes / 1024 / 1024));
  });

  async function saveConfig(next: { enabled?: boolean; maxBytes?: number }): Promise<void> {
    const ok = await cacheStore.setConfig(next);
    if (!ok) toast({ message: t("cache.saveFailed"), tone: "error" });
  }

  async function clearCache(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("cache.clear"),
      body: t("cache.clearBody"),
      confirmLabel: t("common.delete"),
    });
    if (!allowed) return;
    if (await cacheStore.clearAll()) {
      toast({ message: t("cache.cleared"), tone: "success" });
    }
  }

  return (
    <>
      <RmFieldRow label={t("cache.toggle")} hint={<span class="muted-line">{t("cache.tips")}</span>}>
        <RmSwitch checked={stats()?.enabled ?? true} onChange={(v) => void saveConfig({ enabled: v })} />
      </RmFieldRow>
      <RmFieldRow label={t("cache.maxSize")}>
        <div class="btn-row">
          <input
            class="rm-input"
            type="number"
            min={64}
            style={{ width: "130px" }}
            value={maxMiB()}
            onInput={(e) => setMaxMiB(Number(e.currentTarget.value))}
          />
          <span class="muted-line">MiB</span>
          <RmButton size="sm" onClick={() => void saveConfig({ maxBytes: Math.max(64, maxMiB()) * 1024 * 1024 })}>
            {t("common.save")}
          </RmButton>
        </div>
      </RmFieldRow>
      <RmFieldRow label={t("cache.currentSize")}>
        <div class="btn-row">
          <span>
            {formatBytes(stats()?.totalBytes ?? 0)} · {t("cache.entries", { n: stats()?.entries ?? 0 })}
          </span>
          <RmButton size="sm" variant="danger" icon="trash" onClick={() => void clearCache()}>
            {t("cache.clear")}
          </RmButton>
        </div>
      </RmFieldRow>
      <div class="info-note">{t("cache.strategyNote")}</div>
    </>
  );
}

// ── AI & external control (ui-design 07 §2) ──

function AiSection(): JSX.Element {
  const [baseUrl, setBaseUrl] = createSignal("");
  const [model, setModel] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [probeState, setProbeState] = createSignal<"idle" | "ok" | "fail">("idle");

  createEffect(() => {
    if (aiStore.config()) {
      setBaseUrl(aiStore.config()!.baseUrl ?? "");
      setModel(aiStore.config()!.model ?? "");
    }
  });

  async function probe(): Promise<void> {
    const models = await aiStore.probeModels(baseUrl() || undefined, apiKey() || undefined);
    setProbeState(models ? "ok" : "fail");
    if (models && models.length > 0 && !model()) setModel(models[0]);
  }

  /** 模型下拉选项：探测列表 + 兜底包含当前已选/已存模型（自填模型不在列表时也能显示）。 */
  const modelOptions = createMemo(() => {
    const list = aiStore.models().map((m) => ({ value: m, label: m }));
    const current = model().trim();
    if (current && !list.some((o) => o.value === current)) {
      return [{ value: current, label: t("settings.ai.modelSaved", { model: current }) }, ...list];
    }
    return list;
  });

  async function saveAndTest(): Promise<void> {
    const res = await aiStore.saveConfig(
      baseUrl() || undefined,
      model() || undefined,
      apiKey() || undefined
    );
    if (res.ok) {
      setApiKey("");
      const models = await aiStore.probeModels();
      setProbeState(models ? "ok" : "fail");
      toast({ message: t("settings.ai.saved"), tone: "success" });
    } else {
      toast({ message: res.message ?? "", tone: "error" });
    }
  }

  return (
    <div class="tab-stack">
      <div class="settings-ai">
      <RmCard title={t("settings.ai.title")}>
        <RmFieldRow label={t("settings.ai.baseUrl")} hint={<span class="muted-line">{t("settings.ai.baseUrlHint")}</span>}>
          <div class="btn-row">
            <RmInput value={baseUrl()} onInput={setBaseUrl} placeholder="https://api.deepseek.com/v1" mono />
            <RmButton size="sm" icon="search" loading={aiStore.probing()} onClick={() => void probe()}>
              {t("settings.ai.probe")}
            </RmButton>
          </div>
          <Show when={probeState() === "ok"}>
            <div class="muted-line is-ok">✅ {t("settings.ai.probeOk", { n: aiStore.models().length })}</div>
          </Show>
          <Show when={probeState() === "fail"}>
            <div class="rm-field-error">{aiStore.probeError() ?? t("settings.ai.probeFail")}</div>
          </Show>
        </RmFieldRow>
        <RmFieldRow label={t("settings.ai.model")}>
          <Show
            when={modelOptions().length > 0}
            fallback={<RmInput value={model()} onInput={setModel} placeholder={t("settings.ai.modelPlaceholder")} />}
          >
            <RmSelect value={model()} onChange={setModel} options={modelOptions()} />
          </Show>
        </RmFieldRow>
        <RmFieldRow label={t("settings.ai.apiKey")} hint={<span class="muted-line">{t("settings.ai.privacyNote")}</span>}>
          <RmInput
            value={apiKey()}
            onInput={setApiKey}
            type="password"
            placeholder={aiStore.config()?.hasKey ? t("settings.ai.keyConfigured") : t("settings.ai.keyPlaceholder")}
          />
        </RmFieldRow>
        <div class="btn-row">
          <RmButton variant="primary" onClick={() => void saveAndTest()}>
            {t("settings.ai.testAndSave")}
          </RmButton>
          <Show when={aiStore.config()?.configured}>
            <RmTag tone="success">✅ {t("settings.ai.configured")}</RmTag>
          </Show>
        </div>
      </RmCard>
      </div>

      <McpSection />
    </div>
  );
}

function McpSection(): JSX.Element {
  const status = () => mcpStore.status();
  const [showToken, setShowToken] = createSignal(false);
  const [tools, setTools] = createSignal<McpToolView[]>([]);

  createEffect(() => {
    if (status()?.enabled && tools().length === 0) void mcpStore.refreshTools().then(() => setTools(mcpStore.tools()));
  });

  const httpConfig = createMemo(() =>
    JSON.stringify(
      {
        mcpServers: {
          rmccc: {
            url: status()?.endpoint,
            headers: { Authorization: `Bearer ${status()?.token ?? ""}` },
          },
        },
      },
      null,
      2
    )
  );

  const groups = createMemo(() => [
    { risk: "read_only", label: t("settings.mcp.perms.read"), items: tools().filter((tool) => tool.risk === "read_only") },
    { risk: "operate", label: t("settings.mcp.perms.operate"), items: tools().filter((tool) => tool.risk === "operate") },
    { risk: "destructive", label: t("settings.mcp.perms.destructive"), items: tools().filter((tool) => tool.risk === "destructive") },
  ]);

  async function regenerate(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("settings.mcp.regenerate.title"),
      body: t("settings.mcp.regenerate.body"),
      confirmLabel: t("settings.mcp.token.regenerate"),
    });
    if (allowed) {
      await mcpApi.regenerateToken();
      await mcpStore.refreshStatus();
    }
  }

  return (
    <RmCard title={t("settings.mcp.title")}>
      <RmFieldRow label={t("settings.mcp.enable")}>
        <div class="btn-row">
          <RmSwitch
            checked={status()?.enabled ?? false}
            onChange={(v) => void mcpStore.setEnabled(v)}
          />
          <Show when={status()?.enabled}>
            <span class="muted-line">
              {t("settings.mcp.port", { port: String(status()?.port ?? "") })}
            </span>
          </Show>
        </div>
      </RmFieldRow>

      <Show
        when={status()?.enabled}
        fallback={<div class="muted-line">{t("settings.mcp.off")}</div>}
      >
        <RmFieldRow label={t("settings.mcp.token")}>
          <div class="btn-row">
            <code class="rm-input-mono token-display">
              {showToken() ? status()?.token : "•".repeat(12) + (status()?.token ?? "").slice(-4)}
            </code>
            <RmIconButton icon={showToken() ? "eye-off" : "eye"} label={t("settings.mcp.token.show")} onClick={() => setShowToken(!showToken())} />
            <RmIconButton
              icon="copy"
              label={t("settings.mcp.token.copy")}
              onClick={() => {
                void navigator.clipboard.writeText(status()?.token ?? "");
                toast({ message: t("common.copied"), tone: "success" });
              }}
            />
            <RmIconButton icon="refresh" label={t("settings.mcp.token.regenerate")} onClick={() => void regenerate()} />
          </div>
        </RmFieldRow>

        <RmAdvancedSection title={t("settings.mcp.connect")}>
          <div class="btn-row">
            <RmButton
              size="sm"
              icon="copy"
              onClick={() => {
                void navigator.clipboard.writeText(httpConfig());
                toast({ message: t("common.copied"), tone: "success" });
              }}
            >
              {t("settings.mcp.connect.http")}
            </RmButton>
            <RmButton
              size="sm"
              icon="terminal"
              onClick={() => {
                void navigator.clipboard.writeText(JSON.stringify({ mcpServers: { rmccc: { command: "/path/to/rmccc", args: ["mcp"] } } }, null, 2));
                toast({ message: t("common.copied"), tone: "success" });
              }}
            >
              {t("settings.mcp.connect.stdio")}
            </RmButton>
          </div>
          <pre class="logic-result">{httpConfig()}</pre>
        </RmAdvancedSection>

        <RmFieldRow label={t("settings.mcp.sessions")}>
          <Show when={status()?.sessions.length} fallback={<div class="muted-line">{t("settings.mcp.sessions.empty")}</div>}>
            <div class="rm-list">
              <For each={status()?.sessions ?? []}>
                {(session) => (
                  <RmListRow
                    title={`● ${session.client}`}
                    subtitle={session.connectedAt}
                    trailing={
                      <RmButton
                        size="sm"
                        variant="ghost"
                        onClick={async () => {
                          // P2-16: dropping an external session deserves a confirm.
                          const allowed = await uiStore.confirm({
                            title: t("settings.mcp.disconnectConfirm.title"),
                            body: t("settings.mcp.disconnectConfirm.body", { client: session.client }),
                            confirmLabel: t("settings.mcp.disconnect"),
                            danger: true,
                          });
                          if (!allowed) return;
                          await mcpApi.disconnect(session.sessionId);
                          await mcpStore.refreshStatus();
                        }}
                      >
                        {t("settings.mcp.disconnect")}
                      </RmButton>
                    }
                  />
                )}
              </For>
            </div>
          </Show>
        </RmFieldRow>

        <RmAdvancedSection title={t("settings.mcp.perms.title")} summary={`${tools().length}`}>
          <For each={groups()}>
            {(group) => (
              <div class="perm-group">
                <div class="perm-group-title">{group.label} ({group.items.length})</div>
                <For each={group.items}>
                  {(tool) => (
                    <RmListRow
                      title={tool.name}
                      subtitle={tool.description}
                      trailing={
                        <RmSelect
                          value={tool.overridePolicy ?? "default"}
                          onChange={async (v) => {
                            // P2-16: switching an external Agent tool to
                            // allow/deny changes live permissions — confirm it.
                            if (v === "allow" || v === "deny") {
                              const allowed = await uiStore.confirm({
                                title: t("settings.mcp.perms.changeTitle"),
                                body: t("settings.mcp.perms.changeBody", {
                                  tool: tool.name,
                                  policy: t(`settings.mcp.perms.${v}`),
                                }),
                                confirmLabel: t("common.confirm"),
                                danger: v === "deny",
                              });
                              if (!allowed) {
                                // Snap the native select back to the stored value.
                                setTools([]);
                                setTools(mcpStore.tools());
                                return;
                              }
                            }
                            await mcpApi.toolsSet(tool.name, v === "default" ? null : v);
                            await mcpStore.refreshTools().then(() => setTools(mcpStore.tools()));
                          }}
                          options={[
                            { value: "default", label: t("settings.mcp.perms.default", { policy: tool.defaultPolicy }) },
                            { value: "allow", label: t("settings.mcp.perms.allow") },
                            { value: "confirm", label: t("settings.mcp.perms.confirm") },
                            { value: "deny", label: t("settings.mcp.perms.deny") },
                          ]}
                        />
                      }
                    />
                  )}
                </For>
              </div>
            )}
          </For>
        </RmAdvancedSection>

        <RmFieldRow label={t("settings.mcp.recent")}>
          <div class="rm-list">
            <For each={mcpStore.recentCalls().slice(0, 8)}>
              {(call) => (
                <RmListRow
                  title={`${call.sessionId} · ${call.toolId}`}
                  subtitle={call.error ?? ""}
                  trailing={
                    <RmTag tone={call.outcome === "ok" ? "success" : call.outcome === "denied" ? "warning" : "danger"}>
                      {t(`settings.mcp.outcome.${call.outcome}` as TranslationKey)}
                    </RmTag>
                  }
                />
              )}
            </For>
          </div>
        </RmFieldRow>
      </Show>
    </RmCard>
  );
}

// ── Plugins host ──

function PluginsHost(): JSX.Element {
  const navigate = useNavigate();
  onMount(() => void pluginsStore.refresh());
  const hosts = createMemo(() => (pluginsStore.list()?.plugins ?? []).filter((p) => p.settingsPages > 0));
  return (
    <div class="tab-stack">
      <Show when={hosts().length > 0} fallback={
        <RmEmptyState
          icon="extension"
          title={t("settings.plugins.host.empty")}
          action={<RmButton onClick={() => navigate("/plugins")}>{t("settings.plugins.host.manage")}</RmButton>}
        />
      }>
        <For each={hosts()}>
          {(summary) => (
            <RmCard title={`${summary.name} · ${t("settings.plugins.host.provided")}`}>
              <div class="plugin-settings-frame">
                <iframe
                  title={`settings-${summary.id}`}
                  src={`rmcccplugin://${summary.id}/ui/index.html?page=settings`}
                  sandbox="allow-scripts"
                />
              </div>
              <div class="muted-line">{t("settings.plugins.host.permissionNote", { capability: "settings.read" })}</div>
            </RmCard>
          )}
        </For>
      </Show>
    </div>
  );
}

// ── Advanced (version repository) ──

function Advanced(): JSX.Element {
  const [repo, setRepo] = createSignal<InstanceDetail[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  async function refresh(): Promise<void> {
    const res = await instances.list();
    if (res.ok) {
      setRepo(res.data);
      setError(null);
    } else setError(res.error.message);
  }

  onMount(() => void refresh());

  async function rename(instance: InstanceDetail): Promise<void> {
    const next = await promptText({
      title: t("settings.advanced.repo.rename.title"),
      label: t("settings.advanced.repo.rename.prompt"),
      initial: instance.summary.id,
    });
    if (!next || next === instance.summary.id) return;
    const res = await instances.rename(instance.summary.key, next);
    if (res.ok) void refresh();
    else toast({ message: res.error.message, tone: "error" });
  }

  async function duplicate(instance: InstanceDetail): Promise<void> {
    const next = await promptText({
      title: t("settings.advanced.repo.duplicate.title"),
      label: t("settings.advanced.repo.duplicate.prompt"),
      initial: `${instance.summary.id}-copy`,
    });
    if (!next) return;
    const res = await instances.duplicate(instance.summary.key, next, false);
    if (res.ok) void refresh();
    else toast({ message: res.error.message, tone: "error" });
  }

  async function remove(instance: InstanceDetail): Promise<void> {
    const allowed = await confirmDanger({
      title: t("settings.advanced.repo.remove.title"),
      body: t("settings.advanced.repo.remove.body", { id: instance.summary.id }),
      confirmLabel: t("common.delete"),
    });
    if (!allowed) return;
    const res = await instances.remove(instance.summary.key, true);
    if (res.ok) void refresh();
    else toast({ message: res.error.message, tone: "error" });
  }

  return (
    <div class="tab-stack">
      <div class="info-note">{t("settings.advanced.repo.intro")}</div>
      <RmCard
        title={t("settings.advanced.repo.title")}
        actions={
          <RmButton size="sm" icon="refresh" onClick={() => void refresh()}>
            {t("settings.advanced.repo.rescan")}
          </RmButton>
        }
      >
        <Show when={error()} fallback={
          <div class="rm-list">
            <For each={repo() ?? []}>
              {(instance) => (
                <RmListRow
                  title={instance.summary.display_name}
                  subtitle={`${instance.summary.game_version}${instance.summary.loader ? ` · ${loaderLabel(instance.summary.loader.kind)}` : ""}`}
                  trailing={
                    <div class="btn-row">
                      <RmTag tone={instance.directory.kind === "external" ? "success" : "neutral"}>
                        {instance.directory.kind === "external" ? "External" : "Managed"}
                      </RmTag>
                      <RmMenu
                        align="right"
                        items={[
                          { label: t("settings.advanced.repo.rename.title"), icon: "edit", onSelect: () => void rename(instance) },
                          { label: t("settings.advanced.repo.duplicate.title"), icon: "copy", onSelect: () => void duplicate(instance) },
                          { label: t("settings.advanced.repo.remove.title"), icon: "trash", danger: true, onSelect: () => void remove(instance) },
                        ]}
                      >
                        <RmIconButton icon="more-horizontal" label={t("common.more")} />
                      </RmMenu>
                    </div>
                  }
                />
              )}
            </For>
          </div>
        }>
          <RmEmptyState icon="alert" title={error()!} />
        </Show>
      </RmCard>
    </div>
  );
}

// ── About ──

function About(): JSX.Element {
  const [info, setInfo] = createSignal<AppInfoView | null>(null);
  onMount(async () => {
    const res = await settingsApi.appInfo();
    if (res.ok) setInfo(res.data);
  });
  return (
    <div class="tab-stack narrow">
      <RmCard title="SNGL">
        <div class="muted-line" style={{ "margin-bottom": "10px" }}>{t("settings.about.desc")}</div>
        <div class="kv-grid">
          <span>{t("settings.about.launcherVersion")}</span>
          <span>{info()?.launcher_version ?? __APP_VERSION__}</span>
          <span>{t("settings.about.coreVersion")}</span>
          <span>{info()?.core_version ?? "-"}</span>
          <span>{t("settings.about.size")}</span><span>{t("settings.about.sizeValue")}</span>
        </div>
        <div class="btn-row">
          <RmButton
            size="sm"
            icon="external"
            onClick={() => void external.openUrl("https://github.com/chensinuo/RMCL")}
          >
            {t("settings.about.repo")}
          </RmButton>
        </div>
        <div class="about-credits">
          <div class="rm-card-title" style={{ "margin-bottom": "6px" }}>{t("settings.about.credits")}</div>
          <div class="muted-line">
            skinview3d (MIT License) ·{" "}
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault();
                void external.openUrl("https://github.com/bs-community/skinview3d");
              }}
            >
              github.com/bs-community/skinview3d
            </a>
          </div>
          <div class="muted-line">
            three.js (MIT License) ·{" "}
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault();
                void external.openUrl("https://github.com/mrdoob/three.js");
              }}
            >
              github.com/mrdoob/three.js
            </a>
          </div>
        </div>
      </RmCard>
    </div>
  );
}

// ── Storage management ─────────────────────────────────────────────

function StorageSection(): JSX.Element {
  const navigate = useNavigate();
  const [overview, setOverview] = createSignal<StorageOverview | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [cleaning, setCleaning] = createSignal(false);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  async function refresh(): Promise<void> {
    setLoading(true);
    const res = await storage.overview();
    setLoading(false);
    if (res.ok) {
      setOverview(res.data);
      setError(null);
    } else {
      setError(res.error.message);
    }
  }

  onMount(() => void refresh());

  function toggle(id: string): void {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function cleanCache(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("settings.storage.cleanTitle"),
      body: t("settings.storage.cleanBody"),
      confirmLabel: t("settings.storage.clean"),
    });
    if (!allowed) return;
    setCleaning(true);
    const res = await storage.cleanCache();
    setCleaning(false);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    toast({
      message: t("settings.storage.cleaned", {
        size: formatBytes(Number(res.data.freed_bytes)),
      }),
      tone: "success",
    });
    await refresh();
  }

  async function reveal(id: string): Promise<void> {
    const res = await gameDirectories.reveal(id);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  const cleanable = () => Number(overview()?.cleanable_bytes ?? 0);

  return (
    <div class="tab-stack settings-section">
      <RmCard
        title={t("settings.storage.overview")}
        actions={
          <RmButton size="sm" icon="refresh" loading={loading()} onClick={() => void refresh()}>
            {t("common.refresh")}
          </RmButton>
        }
      >
        <Show when={error()}>
          <RmErrorState message={error()!} onRetry={() => void refresh()} />
        </Show>
        <Show when={overview()}>
          {(data) => (
            <>
              <div class="storage-metrics">
                <div class="storage-metric">
                  <span class="storage-metric-label">{t("settings.storage.total")}</span>
                  <span class="storage-metric-value">{formatBytes(Number(data().total_bytes))}</span>
                </div>
                <div class="storage-metric">
                  <span class="storage-metric-label">{t("settings.storage.launcher")}</span>
                  <span class="storage-metric-value">{formatBytes(Number(data().launcher_bytes))}</span>
                </div>
                <div class="storage-metric">
                  <span class="storage-metric-label">{t("settings.storage.games")}</span>
                  <span class="storage-metric-value">{formatBytes(Number(data().game_bytes))}</span>
                </div>
                <div class="storage-metric is-accent">
                  <span class="storage-metric-label">{t("settings.storage.cleanable")}</span>
                  <span class="storage-metric-value">{formatBytes(cleanable())}</span>
                </div>
              </div>
              <div class="btn-row">
                <RmButton
                  variant="primary"
                  icon="trash"
                  loading={cleaning()}
                  disabled={cleanable() === 0}
                  onClick={() => void cleanCache()}
                >
                  {t("settings.storage.clean")}
                </RmButton>
                <span class="muted-line">{t("settings.storage.cleanHint")}</span>
              </div>
            </>
          )}
        </Show>
        <Show when={loading() && !overview()}>
          <div class="storage-skeleton" aria-hidden="true">
            <div /><div /><div />
          </div>
        </Show>
      </RmCard>

      <RmCard title={t("settings.storage.launcherData")}>
        <div class="storage-entry-list">
          <For each={overview()?.entries ?? []}>
            {(entry) => (
              <div class={`storage-entry${entry.cleanable ? " is-cleanable" : ""}`}>
                <Icon name={entry.cleanable ? "trash" : "folder"} size={16} />
                <div class="storage-entry-main">
                  <span class="storage-entry-label">
                    {t(`settings.storage.kind.${entry.kind}` as TranslationKey)}
                    <Show when={entry.cleanable}>
                      <RmTag tone="accent">{t("settings.storage.cleanableTag")}</RmTag>
                    </Show>
                  </span>
                  <span class="storage-entry-path" title={entry.path}>
                    {elidePath(entry.path, 82)}
                  </span>
                </div>
                <span class="storage-entry-size">{formatBytes(Number(entry.bytes))}</span>
              </div>
            )}
          </For>
        </div>
      </RmCard>

      <For each={overview()?.game_directories ?? []}>
        {(dir) => (
          <RmCard
            title={dir.name}
            actions={
              <RmButton size="sm" icon="folder" onClick={() => void reveal(dir.directory_id)}>
                {t("settings.storage.openDir")}
              </RmButton>
            }
          >
            <button class="storage-dir-head" onClick={() => toggle(dir.directory_id)}>
              <Icon name={expanded().has(dir.directory_id) ? "chevron-down" : "chevron-right"} size={15} />
              <span class="storage-dir-path" title={dir.path}>{elidePath(dir.path, 72)}</span>
              <span class="storage-dir-size">{formatBytes(Number(dir.bytes))}</span>
            </button>
            <Show when={expanded().has(dir.directory_id)}>
              <div class="storage-entry-list">
                <div class="storage-entry is-shared">
                  <Icon name="layers" size={16} />
                  <div class="storage-entry-main">
                    <span class="storage-entry-label">{t("settings.storage.shared")}</span>
                    <span class="storage-entry-path">
                      {t("settings.storage.sharedHint")}
                    </span>
                  </div>
                  <span class="storage-entry-size">{formatBytes(Number(dir.shared_bytes))}</span>
                </div>
                <For each={dir.instances}>
                  {(instance) => (
                    <div class="storage-entry">
                      <Icon name="gamepad" size={16} />
                      <div class="storage-entry-main">
                        <span class="storage-entry-label">{instance.display_name}</span>
                        <span class="storage-entry-path" title={instance.path}>
                          {instance.id} · {elidePath(instance.path, 64)}
                        </span>
                      </div>
                      <span class="storage-entry-size">{formatBytes(Number(instance.bytes))}</span>
                      <RmIconButton
                        icon="external"
                        label={t("settings.storage.openInstance")}
                        onClick={() => navigate(`/instances/${encodeURIComponent(instance.key)}`)}
                      />
                    </div>
                  )}
                </For>
                <Show when={dir.instances.length === 0}>
                  <div class="muted-line">{t("settings.storage.noInstances")}</div>
                </Show>
              </div>
            </Show>
          </RmCard>
        )}
      </For>
    </div>
  );
}

declare const __APP_VERSION__: string;
