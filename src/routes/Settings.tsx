import { Component, createSignal, onMount, Show, For } from "solid-js";
import { useNavigate } from "@solidjs/router";
import {
  settings,
  auth,
  logs,
  dashboard,
  type AppConfig,
  type JavaInfo,
  type SettingsUpdate,
  type AccountMetadata,
} from "../ipc/api";
import type { ThemeInfo } from "../ipc/types";
import { listThemes, setThemePreference } from "../theme";
import { showToast } from "../components/Toast";
import {
  t,
  setLocale,
  preference,
  supportedLocales,
  systemLocale,
  nativeNameFor,
} from "../i18n";

/// Splits a textarea into individual arguments, one per line.
function splitLines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

const Settings: Component = () => {
  const navigate = useNavigate();

  // ── State ──
  const [config, setConfig] = createSignal<AppConfig | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);

  const [gameRootDir, setGameRootDir] = createSignal("");
  const [javaPath, setJavaPath] = createSignal("");
  const [minMemory, setMinMemory] = createSignal(256);
  const [maxMemory, setMaxMemory] = createSignal(4096);

  const [javaInstallations, setJavaInstallations] = createSignal<JavaInfo[]>([]);
  const [detectingJava, setDetectingJava] = createSignal(false);

  // ── Global game defaults (fallbacks for instances that inherit) ──
  const [defaultJvmArgs, setDefaultJvmArgs] = createSignal("");
  const [defaultGameArgs, setDefaultGameArgs] = createSignal("");
  const [defaultIsolation, setDefaultIsolation] = createSignal("modded");
  const [defaultWidth, setDefaultWidth] = createSignal(0);
  const [defaultHeight, setDefaultHeight] = createSignal(0);
  const [defaultFullscreen, setDefaultFullscreen] = createSignal(false);
  const [defaultPriority, setDefaultPriority] = createSignal("normal");
  const [autoMemory, setAutoMemory] = createSignal(false);

  const handleThemeChange = async (value: string) => {
    const pref = value === "" ? null : value;
    const res = await setThemePreference(pref);
    if (res.ok) {
      setThemePreferenceState(pref);
      showToast({ code: "success", message: t("settings.theme.saved"), retryable: false });
    } else {
      showToast({ code: "error", message: res.error ?? t("settings.theme.saveFailed"), retryable: false });
    }
  };

  // ── Accounts (0.5.3) ──
  const [accounts, setAccounts] = createSignal<AccountMetadata[]>([]);
  const [accountsLoading, setAccountsLoading] = createSignal(false);
  const [currentAccountId, setCurrentAccountId] = createSignal<string | null>(null);

  // ── Logs (0.5.5) ──
  const [logsText, setLogsText] = createSignal("");
  const [logsLoading, setLogsLoading] = createSignal(false);

  // ── Theme (Phase 3) ──
  const [themePreference, setThemePreferenceState] = createSignal<string | null>(null);
  const [themeOptions, setThemeOptions] = createSignal<ThemeInfo[]>([]);

  onMount(async () => {
    const result = await settings.get();
    if (result.ok) {
      const c = result.data;
      setConfig(c);
      setGameRootDir(c.game_root_dir ?? "");
      setJavaPath(c.default_java_path ?? "");
      setMinMemory(c.default_min_memory_mb);
      setMaxMemory(c.default_max_memory_mb);
      setDefaultJvmArgs(c.default_jvm_args.join("\n"));
      setDefaultGameArgs(c.default_game_args.join("\n"));
      setDefaultIsolation(c.default_isolation);
      setDefaultWidth(c.default_window_width ?? 0);
      setDefaultHeight(c.default_window_height ?? 0);
      setDefaultFullscreen(c.default_fullscreen);
      setDefaultPriority(c.default_process_priority);
      setAutoMemory(c.auto_memory);
    } else {
      showToast(result.error);
    }
    setLoading(false);
    await detectJava();
    await loadAccounts();
    const pref = await dashboard.getThemePreference();
    if (pref.ok) setThemePreferenceState(pref.data);
    setThemeOptions(await listThemes());
  });

  const detectJava = async () => {
    setDetectingJava(true);
    const result = await settings.detectJava();
    if (result.ok) {
      setJavaInstallations(result.data);
    } else {
      showToast(result.error);
    }
    setDetectingJava(false);
  };

  const handleSave = async () => {
    setSaving(true);

    const opts: SettingsUpdate = {
      game_root_dir: gameRootDir().trim() || null,
      default_java_path: javaPath().trim() || null,
      default_min_memory_mb: minMemory(),
      default_max_memory_mb: maxMemory(),
      default_jvm_args: splitLines(defaultJvmArgs()),
      default_game_args: splitLines(defaultGameArgs()),
      default_isolation: defaultIsolation(),
      default_window_width: defaultWidth(),
      default_window_height: defaultHeight(),
      default_fullscreen: defaultFullscreen(),
      default_process_priority: defaultPriority(),
      auto_memory: autoMemory(),
    };

    const result = await settings.set(opts);
    setSaving(false);

    if (result.ok) {
      setConfig(result.data);
      showToast({
        code: "success",
        message: t("settings.saved"),
        retryable: false,
      });
    } else {
      showToast(result.error);
    }
  };

  const selectJava = (path: string) => {
    setJavaPath(path);
  };

  /** "Follow system" label, naming the detected language when we ship it. */
  const followSystemLabel = () => {
    const code = systemLocale();
    const name = nativeNameFor(code);
    return name && name !== code
      ? t("settings.language.systemWith", { language: name })
      : t("settings.language.system");
  };

  const handleLocaleChange = async (e: Event) => {
    const value = (e.currentTarget as HTMLSelectElement).value;
    try {
      // Persist first, then apply, so a failure cannot leave the UI showing a
      // language the backend did not store.
      await setLocale(value === "" ? null : value);
    } catch (err) {
      showToast({ code: "error", message: String(err), retryable: false });
    }
  };

  // ── Accounts (0.5.3) ──
  const loadAccounts = async () => {
    setAccountsLoading(true);
    const r = await auth.listAccounts();
    const session = await auth.getSession();
    setAccountsLoading(false);
    if (r.ok) setAccounts(r.data);
    else showToast(r.error);
    setCurrentAccountId(session.ok && session.data ? session.data.account_id : null);
  };

  const handleSwitchAccount = async (id: string) => {
    const r = await auth.switchAccount(id);
    if (r.ok) {
      setCurrentAccountId(id);
      showToast({ code: "success", message: t("settings.accounts.switched"), retryable: false });
    } else showToast(r.error);
  };

  const handleRemoveAccount = async (id: string) => {
    const r = await auth.removeAccount(id);
    if (r.ok) {
      showToast({ code: "success", message: t("settings.accounts.removed"), retryable: false });
      await loadAccounts();
    } else showToast(r.error);
  };

  const handleRefreshAccounts = async () => {
    const r = await auth.refreshToken();
    if (r.ok) {
      showToast({ code: "success", message: t("settings.accounts.refreshed"), retryable: false });
      await loadAccounts();
    } else showToast(r.error);
  };

  // ── Logs (0.5.5) ──
  const loadLogs = async () => {
    setLogsLoading(true);
    const r = await logs.get(200);
    setLogsLoading(false);
    if (r.ok) setLogsText(r.data.join("\n"));
    else showToast(r.error);
  };

  const clearLogs = async () => {
    const r = await logs.clear();
    if (r.ok) setLogsText("");
    else showToast(r.error);
  };

  // ── Render ──
  return (
    <div class="settings-page">
      <div class="settings-header">
        <button class="settings-back-btn" onClick={() => navigate("/")}>
          {t("common.back")}
        </button>
        <h2 class="settings-title">{t("settings.title")}</h2>
      </div>

      <Show
        when={!loading()}
        fallback={
          <div class="loading-inline">
            <div class="loading-spinner-small" />
            <span>{t("settings.loading")}</span>
          </div>
        }
      >
        <div class="settings-content">
          {/* ── Language ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">{t("settings.language.title")}</h3>
            <p class="settings-section-desc">{t("settings.language.desc")}</p>

            <div class="settings-field">
              <Show
                when={supportedLocales().length > 0}
                fallback={<span class="settings-hint">{t("settings.language.loading")}</span>}
              >
                {/* `selected` per option rather than `value` on the select:
                    Solid applies attributes before `For` creates the options,
                    so a `value` binding would not find its option yet. */}
                <select class="settings-input" onChange={handleLocaleChange}>
                  <option value="" selected={preference() === null}>
                    {followSystemLabel()}
                  </option>
                  <For each={supportedLocales()}>
                    {(loc) => (
                      <option value={loc.code} selected={preference() === loc.code}>
                        {loc.native_name}
                      </option>
                    )}
                  </For>
                </select>
              </Show>
            </div>
          </div>

          {/* ── Theme (Phase 3) ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">{t("settings.theme.title")}</h3>
            <p class="settings-section-desc">{t("settings.theme.desc")}</p>

            <div class="settings-field">
              <select
                class="settings-input"
                onChange={(e) => handleThemeChange(e.currentTarget.value)}
              >
                <option value="" selected={themePreference() === null}>
                  {t("settings.theme.followSystem")}
                </option>
                <option value="builtin:light" selected={themePreference() === "builtin:light"}>
                  {t("settings.theme.light")}
                </option>
                <option value="builtin:dark" selected={themePreference() === "builtin:dark"}>
                  {t("settings.theme.dark")}
                </option>
                <For each={themeOptions().filter((theme) => !theme.isBuiltin)}>
                  {(theme) => (
                    <option
                      value={theme.key}
                      selected={themePreference() === theme.key}
                    >
                      {theme.name}
                    </option>
                  )}
                </For>
              </select>
            </div>
          </div>

          {/* ── Game Directory ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">{t("settings.gameDir.title")}</h3>
            <p class="settings-section-desc">{t("settings.gameDir.desc")}</p>

            <div class="settings-field">
              <label class="settings-label">
                {t("settings.gameDir.label")}
                <span class="settings-hint">
                  {t("settings.gameDir.hint", {
                    default: config()?.game_root_dir || "{data}/instances",
                  })}
                </span>
              </label>
              <input
                type="text"
                class="settings-input settings-input-mono"
                placeholder={t("settings.gameDir.placeholder")}
                value={gameRootDir()}
                onInput={(e) => setGameRootDir(e.currentTarget.value)}
              />
            </div>
          </div>

          {/* ── Java Runtime ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">
              {t("settings.java.title")}
              <button
                class="settings-refresh-btn"
                onClick={detectJava}
                disabled={detectingJava()}
              >
                {detectingJava() ? t("settings.java.detecting") : t("settings.java.redetect")}
              </button>
            </h3>

            <Show
              when={javaInstallations().length > 0}
              fallback={
                <div class="settings-empty">{t("settings.java.empty")}</div>
              }
            >
              <div class="java-list">
                <For each={javaInstallations()}>
                  {(java) => (
                    <div
                      class={`java-item ${javaPath() === java.path ? "selected" : ""}`}
                      onClick={() => selectJava(java.path)}
                    >
                      <div class="java-version-badge">
                        {t("settings.java.badge", { version: java.major_version })}
                      </div>
                      <div class="java-info">
                        <div class="java-path">{java.path}</div>
                        <div class="java-meta">
                          {java.vendor}
                          {java.is_64bit ? t("settings.java.arch64") : t("settings.java.arch32")}
                        </div>
                      </div>
                      <Show when={javaPath() === java.path}>
                        <div class="java-check">✓</div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <div class="settings-field" style="margin-top: 12px">
              <label class="settings-label">
                {t("settings.java.customLabel")}
                <span class="settings-hint">
                  {t("settings.java.customHint")}
                </span>
              </label>
              <input
                type="text"
                class="settings-input settings-input-mono"
                placeholder={t("settings.java.customPlaceholder")}
                value={javaPath()}
                onInput={(e) => setJavaPath(e.currentTarget.value)}
              />
            </div>
          </div>

          {/* ── Global game defaults ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">{t("settings.globalGame.title")}</h3>
            <p class="settings-section-desc">{t("settings.globalGame.desc")}</p>

            <div class="settings-field">
              <label class="settings-label">
                {t("settings.globalGame.isolation")}
                <span class="settings-hint">{t("instanceSettings.isolation.subtitle")}</span>
              </label>
              <select
                class="settings-input"
                onChange={(e) => setDefaultIsolation(e.currentTarget.value)}
              >
                <option value="never" selected={defaultIsolation() === "never"}>
                  {t("instanceSettings.isolation.never")}
                </option>
                <option value="modded" selected={defaultIsolation() === "modded"}>
                  {t("instanceSettings.isolation.modded")}
                </option>
                <option value="always" selected={defaultIsolation() === "always"}>
                  {t("instanceSettings.isolation.always")}
                </option>
              </select>
            </div>

            <div class="settings-field">
              <label class="settings-label">
                {t("settings.globalGame.jvmArgs")}
                <span class="settings-hint">{t("settings.globalGame.onePerLine")}</span>
              </label>
              <textarea
                class="settings-input settings-input-mono"
                rows="3"
                value={defaultJvmArgs()}
                onInput={(e) => setDefaultJvmArgs(e.currentTarget.value)}
              />
            </div>

            <div class="settings-field">
              <label class="settings-label">
                {t("settings.globalGame.gameArgs")}
                <span class="settings-hint">{t("settings.globalGame.onePerLine")}</span>
              </label>
              <textarea
                class="settings-input settings-input-mono"
                rows="3"
                value={defaultGameArgs()}
                onInput={(e) => setDefaultGameArgs(e.currentTarget.value)}
              />
            </div>

            <div class="memory-row">
              <div class="settings-field memory-field">
                <label class="settings-label">{t("settings.globalGame.windowWidth")}</label>
                <input
                  type="number"
                  class="settings-input"
                  min="0"
                  value={defaultWidth()}
                  onInput={(e) => setDefaultWidth(Number(e.currentTarget.value))}
                />
              </div>
              <div class="settings-field memory-field">
                <label class="settings-label">{t("settings.globalGame.windowHeight")}</label>
                <input
                  type="number"
                  class="settings-input"
                  min="0"
                  value={defaultHeight()}
                  onInput={(e) => setDefaultHeight(Number(e.currentTarget.value))}
                />
              </div>
            </div>

            <div class="settings-field">
              <label class="settings-label">{t("settings.globalGame.priority")}</label>
              <select
                class="settings-input"
                onChange={(e) => setDefaultPriority(e.currentTarget.value)}
              >
                <option value="low" selected={defaultPriority() === "low"}>
                  {t("instanceSettings.window.priority.low")}
                </option>
                <option value="below_normal" selected={defaultPriority() === "below_normal"}>
                  {t("instanceSettings.window.priority.below_normal")}
                </option>
                <option value="normal" selected={defaultPriority() === "normal"}>
                  {t("instanceSettings.window.priority.normal")}
                </option>
                <option value="above_normal" selected={defaultPriority() === "above_normal"}>
                  {t("instanceSettings.window.priority.above_normal")}
                </option>
                <option value="high" selected={defaultPriority() === "high"}>
                  {t("instanceSettings.window.priority.high")}
                </option>
              </select>
            </div>

            <label class="modal-checkbox">
              <input
                type="checkbox"
                checked={defaultFullscreen()}
                onChange={(e) => setDefaultFullscreen(e.currentTarget.checked)}
              />
              {t("settings.globalGame.fullscreen")}
            </label>
            <label class="modal-checkbox">
              <input
                type="checkbox"
                checked={autoMemory()}
                onChange={(e) => setAutoMemory(e.currentTarget.checked)}
              />
              {t("settings.globalGame.autoMemory")}
            </label>
          </div>

          {/* ── Memory ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">{t("settings.memory.title")}</h3>
            <p class="settings-section-desc">{t("settings.memory.desc")}</p>

            <div class="memory-row">
              <div class="settings-field memory-field">
                <label class="settings-label">
                  {t("settings.memory.min")}
                  <span class="settings-hint">{t("settings.memory.unit")}</span>
                </label>
                <input
                  type="number"
                  class="settings-input"
                  min="128"
                  max="65536"
                  step="128"
                  value={minMemory()}
                  onInput={(e) => setMinMemory(Number(e.currentTarget.value))}
                />
              </div>
              <div class="settings-field memory-field">
                <label class="settings-label">
                  {t("settings.memory.max")}
                  <span class="settings-hint">MB</span>
                </label>
                <input
                  type="number"
                  class="settings-input"
                  min="256"
                  max="65536"
                  step="256"
                  value={maxMemory()}
                  onInput={(e) => setMaxMemory(Number(e.currentTarget.value))}
                />
              </div>
            </div>
          </div>

          {/* ── Accounts (0.5.3) ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">
              {t("settings.accounts.title")}
              <button class="settings-refresh-btn" onClick={loadAccounts} disabled={accountsLoading()}>
                {accountsLoading() ? t("settings.loading") : t("settings.accounts.refresh")}
              </button>
            </h3>
            <p class="settings-section-desc">{t("settings.accounts.desc")}</p>
            <Show when={accounts().length > 0} fallback={<div class="settings-empty">{t("settings.accounts.empty")}</div>}>
              <div class="java-list">
                <For each={accounts()}>
                  {(acc) => (
                    <div class={`java-item ${currentAccountId() === acc.account_id ? "selected" : ""}`}>
                      <div class="java-info">
                        <div class="java-path">{acc.profile_name}</div>
                        <div class="java-meta">{acc.type} · {acc.profile_id}</div>
                      </div>
                      <div class="modal-actions">
                        <button
                          class="modal-btn-primary"
                          disabled={currentAccountId() === acc.account_id}
                          onClick={() => handleSwitchAccount(acc.account_id)}
                        >
                          {currentAccountId() === acc.account_id ? t("settings.accounts.current") : t("settings.accounts.switch")}
                        </button>
                        <button class="modal-btn-cancel" onClick={() => handleRemoveAccount(acc.account_id)}>
                          {t("settings.accounts.remove")}
                        </button>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
            <div class="modal-actions">
              <button class="modal-btn-cancel" onClick={handleRefreshAccounts}>
                {t("settings.accounts.refreshToken")}
              </button>
            </div>
          </div>

          {/* ── Logs (0.5.5) ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">
              {t("settings.logs.title")}
              <button class="settings-refresh-btn" onClick={loadLogs} disabled={logsLoading()}>
                {logsLoading() ? t("settings.loading") : t("settings.logs.load")}
              </button>
            </h3>
            <p class="settings-section-desc">{t("settings.logs.desc")}</p>
            <Show when={logsText() !== ""} fallback={<div class="settings-empty">{t("settings.logs.empty")}</div>}>
              <pre class="mod-log-pre">{logsText()}</pre>
            </Show>
            <div class="modal-actions">
              <button class="modal-btn-cancel" onClick={clearLogs}>{t("settings.logs.clear")}</button>
            </div>
          </div>

          {/* ── Save ── */}
          <div class="settings-actions">
            <button
              class="settings-save-btn"
              onClick={handleSave}
              disabled={saving()}
            >
              {saving() ? t("settings.saving") : t("settings.save")}
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Settings;
