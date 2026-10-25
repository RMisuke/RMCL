/**
 * Settings (ui-design 08): seven hash sections (game defaults / general /
 * download / AI & control / plugins host / advanced / about).
 * The #ai section implements ui-design 07 §2 (AI service + MCP management).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, Switch, Match } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmSwitch, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmCard, RmSideTabs, RmListRow, RmAdvancedSection, RmFieldRow, RmEmptyState } from "../components/rm/surfaces";
import { settings as settingsApi, userSettings, instances, logs, external, mcp as mcpApi, ai as aiApi, plugins as pluginsApi, dashboard } from "../ipc/api";
import type { JavaInfo, InstanceInfo, LocaleInfo, McpToolView } from "../ipc/types";
import { settingsStore } from "../stores/settings";
import { aiStore } from "../stores/ai";
import { mcpStore } from "../stores/mcp";
import { pluginsStore } from "../stores/plugins";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { listThemes } from "../theme";
import { cacheStore } from "../stores/cache";
import { formatBytes } from "../lib/format";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { elidePath } from "../lib/format";

const SECTIONS: { id: string; icon: "list" | "settings" | "download" | "bot" | "extension" | "wrench" | "info" | "sparkles"; label: TranslationKey }[] = [
  { id: "game-defaults", icon: "list", label: "settings.nav.gameDefaults" },
  { id: "general", icon: "settings" as const, label: "settings.nav.general" },
  { id: "appearance", icon: "sparkles" as const, label: "settings.nav.appearance" },
  { id: "download", icon: "download" as const, label: "settings.nav.download" },
  { id: "ai", icon: "bot" as const, label: "settings.nav.ai" },
  { id: "plugins", icon: "extension" as const, label: "settings.nav.plugins" },
  { id: "advanced", icon: "wrench" as const, label: "settings.nav.advanced" },
  { id: "about", icon: "info" as const, label: "settings.nav.about" },
];

export default function Settings(): JSX.Element {
  const location = useLocation();
  const [section, setSection] = createSignal(location.hash.replace("#", "") || "game-defaults");

  createEffect(() => {
    history.replaceState(null, "", `#${section()}`);
  });

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
  const config = () => settingsStore.config();
  const [rootDir, setRootDir] = createSignal("");
  const [javas, setJavas] = createSignal<JavaInfo[]>([]);
  const [autoMemory, setAutoMemory] = createSignal(false);

  createEffect(() => {
    if (config()) {
      setRootDir(config()!.game_root_dir ?? "");
      setAutoMemory(config()!.auto_memory);
    }
  });

  onMount(() => void settingsApi.detectJava().then((r) => r.ok && setJavas(r.data)));

  async function patch(update: Parameters<typeof settingsStore.update>[0]): Promise<void> {
    const res = await settingsStore.update(update);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  return (
    <div class="tab-stack">
      <div class="info-note">{t("settings.game.intro")}</div>
      <RmCard title={t("settings.game.rootDir")}>
        <div class="btn-row">
          <RmInput value={rootDir()} onInput={setRootDir} placeholder={t("settings.game.rootDirPlaceholder")} />
          <RmButton size="sm" variant="primary" onClick={() => void patch({ game_root_dir: rootDir().trim() || null })}>
            {t("common.save")}
          </RmButton>
        </div>
        <div class="muted-line">{t("settings.game.rootDirNote")}</div>
      </RmCard>

      <RmCard title={t("settings.game.java")}>
        <RmFieldRow label={t("settings.game.javaDefault")}>
          <RmSelect
            value={config()?.default_java_path ?? ""}
            onChange={(v) => void patch({ default_java_path: v || null })}
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
            value={config()?.default_isolation ?? "modded"}
            onChange={(v) => void patch({ default_isolation: v })}
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
              void patch({ auto_memory: v });
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
                value={config()?.default_min_memory_mb ?? 1024}
                onInput={(e) => void patch({ default_min_memory_mb: Number(e.currentTarget.value) })}
              />
              <span>–</span>
              <input
                class="rm-input"
                type="number"
                style={{ width: "110px" }}
                value={config()?.default_max_memory_mb ?? 4096}
                onInput={(e) => void patch({ default_max_memory_mb: Number(e.currentTarget.value) })}
              />
              <span>MiB</span>
            </div>
          </RmFieldRow>
        </Show>
      </RmCard>

      <RmCard title={t("settings.game.window")}>
        <div class="btn-row">
          <input class="rm-input" type="number" style={{ width: "100px" }} value={config()?.default_window_width ?? 854} onInput={(e) => void patch({ default_window_width: Number(e.currentTarget.value) })} />
          ×
          <input class="rm-input" type="number" style={{ width: "100px" }} value={config()?.default_window_height ?? 480} onInput={(e) => void patch({ default_window_height: Number(e.currentTarget.value) })} />
          <RmSwitch checked={config()?.default_fullscreen ?? false} onChange={(v) => void patch({ default_fullscreen: v })} />
          <span>{t("instanceDetail.settings.fullscreen")}</span>
        </div>
      </RmCard>

      <RmAdvancedSection title={t("settings.game.advancedArgs")} summary={t("instanceDetail.settings.argsSummary")}>
        <RmFieldRow label={t("settings.game.jvmArgs")}>
          <textarea
            class="rm-textarea rm-input-mono"
            rows={3}
            value={(config()?.default_jvm_args ?? []).join("\n")}
            onInput={(e) => void patch({ default_jvm_args: e.currentTarget.value.split("\n").map((l) => l.trim()).filter(Boolean) })}
          />
        </RmFieldRow>
        <RmFieldRow label={t("settings.game.gameArgs")}>
          <textarea
            class="rm-textarea rm-input-mono"
            rows={2}
            value={(config()?.default_game_args ?? []).join("\n")}
            onInput={(e) => void patch({ default_game_args: e.currentTarget.value.split("\n").map((l) => l.trim()).filter(Boolean) })}
          />
        </RmFieldRow>
      </RmAdvancedSection>
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
        <div class="btn-row">
          <RmSwitch checked={settingsStore.user()?.check_updates ?? false} onChange={(v) => void settingsStore.updateUser({ check_updates: v, locale: null, theme: null })} />
          <span>{t("settings.general.updates.autoCheck")}</span>
          <RmButton size="sm" disabled title={t("settings.general.updates.soon")}>
            {t("settings.general.updates.checkNow")}
          </RmButton>
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

  return (
    <div class="tab-stack narrow">
      <RmCard title={t("settings.general.appearance")}>
        {/* 「跟随系统」已按需求移除：只保留显式选择的主题卡片 */}
        <div class="theme-grid">
          <For each={themes()}>
            {(info) => (
              <button
                class={`theme-card${settingsStore.themePreference() === info.key ? " is-active" : ""}`}
                onClick={() =>
                  void settingsStore.setTheme(info.key).then((res) => {
                    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
                  })
                }
              >
                <ThemeSwatch key_={info.key} />
                <span>{info.name}</span>
              </button>
            )}
          </For>
        </div>
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

/** 4-color palette strip from `dashboard_get_theme_tokens` (ui-design 01 §3). */
function ThemeSwatch(props: { key_: string }): JSX.Element {
  const [tokens, setTokens] = createSignal<{ background: string; surface: string; accent: string; text: string } | null>(null);
  createEffect(() => {
    void import("../ipc/api").then(({ dashboard }) =>
      dashboard.getThemeTokens(props.key_).then((res) => {
        if (res.ok) setTokens(res.data);
      })
    );
  });
  return (
    <div class="theme-swatch">
      <Show when={tokens()}>
        {(tk) => (
          <>
            <span style={{ background: tk().background }} />
            <span style={{ background: tk().surface }} />
            <span style={{ background: tk().accent }} />
            <span style={{ background: tk().text }} />
          </>
        )}
      </Show>
    </div>
  );
}

// ── Download settings ──

function DownloadSection(): JSX.Element {
  const config = () => settingsStore.config();
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
            value={config()?.download_concurrency ?? 16}
            onInput={(e) => void settingsStore.update({ download_concurrency: Number(e.currentTarget.value) })}
          />
          <span class="muted-line">{t("settings.download.concurrencyHint")}</span>
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
      return [{ value: current, label: `${current} (saved)` }, ...list];
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
                      {call.outcome}
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
  onMount(() => void pluginsStore.refresh());
  const hosts = createMemo(() => (pluginsStore.list()?.plugins ?? []).filter((p) => p.settingsPages > 0));
  return (
    <div class="tab-stack">
      <Show when={hosts().length > 0} fallback={
        <RmEmptyState
          icon="extension"
          title={t("settings.plugins.host.empty")}
          action={<RmButton onClick={() => (window.location.hash = "#/plugins")}>{t("settings.plugins.host.manage")}</RmButton>}
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
  const [repo, setRepo] = createSignal<InstanceInfo[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  async function refresh(): Promise<void> {
    const res = await instances.list();
    if (res.ok) {
      setRepo(res.data);
      setError(null);
    } else setError(res.error.message);
  }

  onMount(() => void refresh());

  async function rename(instance: InstanceInfo): Promise<void> {
    const next = await promptText({
      title: t("settings.advanced.repo.rename.title"),
      label: t("settings.advanced.repo.rename.prompt"),
      initial: instance.id,
    });
    if (!next || next === instance.id) return;
    const res = await instances.rename(instance.id, next);
    if (res.ok) void refresh();
    else toast({ message: res.error.message, tone: "error" });
  }

  async function duplicate(instance: InstanceInfo): Promise<void> {
    const next = await promptText({
      title: t("settings.advanced.repo.duplicate.title"),
      label: t("settings.advanced.repo.duplicate.prompt"),
      initial: `${instance.id}-copy`,
    });
    if (!next) return;
    const res = await instances.duplicate(instance.id, next);
    if (res.ok) void refresh();
    else toast({ message: res.error.message, tone: "error" });
  }

  async function remove(instance: InstanceInfo): Promise<void> {
    const allowed = await confirmDanger({
      title: t("settings.advanced.repo.remove.title"),
      body: t("settings.advanced.repo.remove.body", { id: instance.id }),
      confirmLabel: t("common.delete"),
    });
    if (!allowed) return;
    const res = await instances.remove(instance.id);
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
                  title={instance.id}
                  subtitle={`${instance.game_version}${instance.modded ? " · modded" : ""}`}
                  trailing={
                    <div class="btn-row">
                      <RmTag tone={instance.missing_count === 0 ? "success" : "warning"}>
                        {instance.missing_count === 0 ? t("settings.advanced.repo.complete") : t("settings.advanced.repo.missing", { n: instance.missing_count })}
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
  const version = __APP_VERSION__;
  return (
    <div class="tab-stack narrow">
      <RmCard title="RMCCC">
        <div class="kv-grid">
          <span>{t("settings.about.version")}</span><span>{version}</span>
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

declare const __APP_VERSION__: string;
