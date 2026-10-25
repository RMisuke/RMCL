/**
 * IPC API wrapper — all invoke calls go through here.
 *
 * P2-15：类型由 ts-rs 自动生成于 `./types`（src-tauri/src/bindings.rs 导出），
 * 此处仅导入并 re-export，避免手动维护 interface 与后端字段不同步。
 */

import { invoke } from "@tauri-apps/api/core";

// ── 自动生成的类型桥（ts-rs）──
export type {
  AuthSession,
  AccountMetadata,
  Profile,
  ModLoaderConfig,
  ModLoaderType,
  ModVersion,
  VersionInfo,
  ReleaseType,
  AppConfig,
  UserSettings,
  UserSettingsUpdate,
  LocaleInfo,
  IpcError,
  ErrorCode,
  ResolvedInstanceSettings,
  JavaInfo,
  InstanceInfo,
  MissingFileInfo,
  InstancePaths,
  DownloadProgressPayload,
  PluginList,
  PluginDetail,
  InstallReport,
  PluginSummary,
  PluginConfigView,
  PluginConfig,
  CapabilityInfo,
  PluginContributions,
  DashboardLayout,
  NormalizationResult,
  WidgetDescriptor,
  ThemeInfo,
  ThemeTokens,
  WidgetInstance,
  BridgeReply,
  AiConfigView,
  AiAnalysisHandle,
  ModsDependencyScanView,
  AppliedPlanReport,
  McpStatusView,
  McpToolView,
  McpConfirmationView,
  McpRecentCallView,
} from "./types";

import type { AppInfoView,
  SettingsUpdate as GeneratedSettingsUpdate,
  ProfileUpdateOpts as GeneratedProfileUpdateOpts,
  AuthSession,
  AccountMetadata,
  Profile,
  ModVersion,
  ModInfo,
  ModSearchResult,
  InstanceInfo,
  VersionInfo,
  AppConfig,
  UserSettings,
  UserSettingsUpdate,
  LocaleInfo,
  IpcError,
  JavaInfo,
  MissingFileInfo,
  InstancePaths,
  ModList,
  LoaderVersion,
  ResolvedInstanceSettings,
  PluginList,
  PluginDetail,
  InstallReport,
  PluginSummary,
  PluginConfigView,
  PluginConfig,
  CapabilityInfo,
  PluginContributions,
  DashboardLayout,
  NormalizationResult,
  WidgetDescriptor,
  ThemeInfo,
  ThemeTokens,
  WidgetInstance,
  BridgeReply,
  AiConfigView,
  AiAnalysisHandle,
  ModsDependencyScanView,
  AppliedPlanReport,
  McpStatusView,
  McpToolView,
  McpConfirmationView,
  McpRecentCallView,
} from "./types";

// Input DTOs: partial updates. The fields come from the ts-rs generated types
// (so they can never drift from the Rust structs), wrapped in `Partial` because
// the backend treats an omitted field as "leave unchanged".
export type SettingsUpdate = Partial<GeneratedSettingsUpdate>;
export type ProfileUpdateOpts = Partial<GeneratedProfileUpdateOpts>;

// ── Safe invoke wrapper ──

export async function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<{ ok: true; data: T } | { ok: false; error: IpcError }> {
  try {
    const data = await invoke<T>(cmd, args);
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: e as IpcError };
  }
}

// ── Auth ──

export const auth = {
  microsoftLogin: () => safeInvoke<AuthSession>("auth_microsoft_login"),
  offlineLogin: (username: string) =>
    safeInvoke<AuthSession>("auth_offline_login", { username }),
  logout: () => safeInvoke<void>("auth_logout"),
  getSession: () => safeInvoke<AuthSession | null>("auth_get_session"),
  refreshToken: () => safeInvoke<AuthSession | null>("auth_refresh_token"),
  listAccounts: () => safeInvoke<AccountMetadata[]>("auth_list_accounts"),
  switchAccount: (accountId: string) =>
    safeInvoke<AuthSession>("auth_switch_account", { accountId }),
  removeAccount: (accountId: string) =>
    safeInvoke<void>("auth_remove_account", { accountId }),
  /** Skin + cape texture URLs via the backend session-server proxy (null = none). */
  skinUrl: (uuid: string) =>
    safeInvoke<import("./types").SkinTextures | null>("auth_skin_url", { uuid }),
};

// ── Profiles ──

export const profiles = {
  list: () => safeInvoke<Profile[]>("profile_list"),
  create: (name: string, gameVersion: string) =>
    safeInvoke<Profile>("profile_create", { name, gameVersion }),
  update: (id: string, opts: ProfileUpdateOpts) =>
    safeInvoke<Profile>("profile_update", { id, opts }),
  delete: (id: string) => safeInvoke<void>("profile_delete", { id }),
};

// ── Versions ──

export const versions = {
  list: () => safeInvoke<VersionInfo[]>("version_list"),
  install: (version: string, profileId: string) =>
    safeInvoke<void>("version_install", { version, profileId }),
};

// ── Launch ──

export const launch = {
  start: (profileId: string) => safeInvoke<void>("launch_start", { profileId }),
  kill: (profileId: string) => safeInvoke<void>("launch_kill", { profileId }),
  status: () => safeInvoke<string[]>("launch_status"),
};

// ── Mods ──

export const mods = {
  search: (
    query: string,
    gameVersion?: string,
    loader?: string,
    page?: number,
    limit?: number,
    index?: string
  ) =>
    safeInvoke<ModSearchResult>("mod_search", { query, gameVersion, loader, page, limit, index }),
  getVersions: (modId: string) =>
    safeInvoke<ModVersion[]>("mod_get_versions", { modId }),
  getDownloadUrl: (versionId: string) =>
    safeInvoke<string>("mod_get_download_url", { versionId }),
  list: (profileId: string) =>
    safeInvoke<ModList>("mod_list", { profileId }),
  toggle: (profileId: string, modFilename: string, enabled: boolean) =>
    safeInvoke<void>("mod_toggle", { profileId, modFilename, enabled }),
  install: (profileId: string, version: ModVersion) =>
    safeInvoke<string>("mod_install", { profileId, version }),
  remove: (profileId: string, modFilename: string) =>
    safeInvoke<void>("mod_remove", { profileId, modFilename }),
  pickAndInstall: (profileId: string) =>
    safeInvoke<string | null>("mod_pick_and_install", { profileId }),
  /** 检查缺失前置（ADR-013 S3，纯逻辑无 AI）：扫描报告 + dry-run 补装计划。 */
  dependencyScan: (profileId: string) =>
    safeInvoke<ModsDependencyScanView>("mods_dependency_scan", { profileId }),
  /** 一键补装：执行前后端会重新扫描（Core 再校验），只装仍缺失项。 */
  applyPlan: (profileId: string) =>
    safeInvoke<AppliedPlanReport>("mods_apply_plan", { profileId }),
};

// ── Mod loaders ──

export const modLoader = {
  list: (loaderType: string, gameVersion: string) =>
    safeInvoke<LoaderVersion[]>("modloader_list", { loaderType, gameVersion }),
  install: (profileId: string, loaderType: string, loaderVersion: string) =>
    safeInvoke<void>("modloader_install", { profileId, loaderType, loaderVersion }),
};

// ── Downloads ──

export const downloads = {
  cancel: (batchId: string) => safeInvoke<void>("download_cancel", { batchId }),
  pause: (batchId: string) => safeInvoke<void>("download_pause", { batchId }),
  resume: (batchId: string) => safeInvoke<void>("download_resume", { batchId }),
};

// ── Instances（本地实例仓库 / 路径）──

export const instances = {
  /** Local version repository entries (disk `versions/` — NOT profiles).
   *  `root` selects an extra game root; omit for the primary root. */
  list: (root?: string) =>
    safeInvoke<InstanceInfo[]>("instance_list", { root }),
  /** Effective on-disk locations; the run dir depends on the isolation setting. */
  paths: (id: string) => safeInvoke<InstancePaths>("instance_paths", { id }),
  refresh: () => safeInvoke<InstanceInfo[]>("instance_refresh"),
  checkCompletion: (id: string) =>
    safeInvoke<MissingFileInfo[]>("instance_check_completion", { id }),
  /** Returns the number of repaired files. */
  repair: (id: string) => safeInvoke<number>("instance_repair", { id }),
  /** Version-repository maintenance (ids are VERSION ids, not profile ids). */
  rename: (id: string, newId: string) =>
    safeInvoke<void>("instance_rename", { id, newId }),
  duplicate: (id: string, newId: string) =>
    safeInvoke<void>("instance_duplicate", { id, newId }),
  remove: (id: string) => safeInvoke<void>("instance_remove", { id }),
  /**
   * Effective launch values (instance overrides merged with launcher defaults).
   * Single source of truth for "what will actually be used" — the settings page
   * must not re-derive inheritance in TypeScript (audit P2-8).
   */
  resolvedSettings: (id: string) =>
    safeInvoke<ResolvedInstanceSettings>("instance_resolved_settings", { id }),
};

// ── Settings ──

export const settings = {
  get: () => safeInvoke<AppConfig>("settings_get"),
  set: (opts: SettingsUpdate) => safeInvoke<AppConfig>("settings_set", { opts }),
  detectJava: () => safeInvoke<JavaInfo[]>("java_detect"),
  appInfo: () =>
    safeInvoke<AppInfoView>("settings_app_info", {}),
};

// ── User-level settings（UI 语言等）──

export const userSettings = {
  get: () => safeInvoke<UserSettings>("settings_get_user"),
  /** `locale: ""` = follow the system language; a tag pins it; `null` = unchanged. */
  set: (opts: UserSettingsUpdate) =>
    safeInvoke<UserSettings>("settings_set_user", { opts }),
  supportedLocales: () => safeInvoke<LocaleInfo[]>("settings_supported_locales"),
};

// ── Logs（P3-01：补 log_get/log_clear wrapper）──

export const logs = {
  get: (lines?: number) => safeInvoke<string[]>("log_get", { lines }),
  clear: () => safeInvoke<void>("log_clear"),
};

// ── External ──

export const external = {
  openUrl: (url: string) => safeInvoke<void>("open_external_url", { url }),
};

// ── 图标缓存（rmcccicon:// 协议 + stale-while-revalidate）──

export const cache = {
  /** 条件刷新一个图标；返回 hit/updated/unchanged/stale/miss/disabled。 */
  iconFetch: (url: string, kind?: string) =>
    safeInvoke<import("./types").IconFetchStatus>("cache_icon_fetch", { url, kind }),
  stats: () => safeInvoke<import("./types").IconCacheStats>("cache_icon_stats"),
  setConfig: (enabled?: boolean, maxBytes?: number) =>
    safeInvoke<import("./types").IconCacheStats>("cache_icon_set_config", { enabled, maxBytes }),
  clear: () => safeInvoke<import("./types").IconCacheStats>("cache_icon_clear"),
};

// ── Native dialogs ──

export const dialog = {
  /** Native directory picker; null on cancel. */
  pickDirectory: () => safeInvoke<string | null>("dialog_pick_directory"),
};

// ── Plugins（Phase 2 声明式插件）──

export const plugins = {
  list: (includeRemoved = false) =>
    safeInvoke<PluginList>("plugin_list", { includeRemoved }),
  inspect: (id: string) => safeInvoke<PluginDetail>("plugin_inspect", { id }),
  install: (path: string) => safeInvoke<InstallReport>("plugin_install", { path }),
  pickAndInstall: () =>
    safeInvoke<InstallReport | null>("plugin_pick_and_install"),
  enable: (id: string) => safeInvoke<PluginSummary>("plugin_enable", { id }),
  disable: (id: string) => safeInvoke<PluginSummary>("plugin_disable", { id }),
  remove: (id: string, deleteData = false) =>
    safeInvoke<void>("plugin_remove", { id, deleteData }),
  rollback: (id: string) => safeInvoke<PluginSummary>("plugin_rollback", { id }),
  grant: (id: string, capability: string) =>
    safeInvoke<PluginSummary>("plugin_grant", { id, capability }),
  revoke: (id: string, capability: string) =>
    safeInvoke<PluginSummary>("plugin_revoke", { id, capability }),
  configGet: (id: string) =>
    safeInvoke<PluginConfigView>("plugin_config_get", { id }),
  configSet: (id: string, values: unknown) =>
    safeInvoke<PluginConfig>("plugin_config_set", { id, values }),
  configReset: (id: string) =>
    safeInvoke<PluginConfig>("plugin_config_reset", { id }),
  capabilities: () =>
    safeInvoke<CapabilityInfo[]>("plugin_capabilities"),
  contributions: () =>
    safeInvoke<PluginContributions>("plugin_contributions"),
};

// ── Dashboard layout & themes（Phase 3）──

export const dashboard = {
  getLayout: (layoutId?: string) =>
    safeInvoke<DashboardLayout>("dashboard_get_layout", { layoutId }),
  saveLayout: (layout: DashboardLayout) =>
    safeInvoke<NormalizationResult>("dashboard_save_layout", { layout }),
  resetLayout: (layoutId?: string) =>
    safeInvoke<DashboardLayout>("dashboard_reset_layout", { layoutId }),
  listWidgets: () =>
    safeInvoke<WidgetDescriptor[]>("dashboard_list_widgets"),
  listThemes: () => safeInvoke<ThemeInfo[]>("dashboard_list_themes"),
  getThemeTokens: (key: string) =>
    safeInvoke<ThemeTokens>("dashboard_get_theme_tokens", { key }),
  setTheme: (theme: string) =>
    safeInvoke<UserSettings>("dashboard_set_theme_preference", { theme }),
  getThemePreference: () =>
    safeInvoke<string | null>("dashboard_get_theme_preference"),
  getThemeJson: (key: string) =>
    safeInvoke<unknown>("dashboard_get_theme_json", { key }),
};

// ── WebView diagnostics ──

/**
 * Forward a frontend-side message to the backend log.
 *
 * Release builds have no Web Inspector, so a caught error would otherwise leave
 * no trace. `index.html`'s boot guard reports through the same command without
 * importing this module (it must work when the bundle fails to load).
 */
export const reportFrontendMessage = (
  message: string,
  level: "trace" | "debug" | "info" | "warn" | "error" = "error"
) => safeInvoke<null>("frontend_log", { level, message });

// ── Logic plugins（Phase 5，ADR-007）──

export const logic = {
  /** Run a read-only logic call; the host re-checks `logic.invoke` every time. */
  invoke: (pluginId: string, call: string, input?: unknown, timeoutMs?: number) =>
    safeInvoke<unknown>("plugin_logic_invoke", { pluginId, call, input, timeoutMs }),
  /** Kill the plugin's helper, aborting any in-flight call. */
  cancel: (pluginId: string) => safeInvoke<boolean>("plugin_logic_cancel", { pluginId }),
};

// ── Isolated plugin UI（Phase 4，ADR-006）──

export const ai = {
  getConfig: () => safeInvoke<AiConfigView>("ai_config_get"),
  /** All fields optional: blank = keep the stored value (backend merge). */
  setConfig: (baseUrl?: string, model?: string, apiKey?: string) =>
    safeInvoke<void>("ai_config_set", { baseUrl, model, apiKey }),
  /** 模型探测：`GET {base}/models`。传参即可在保存前探测；不传用已存配置。 */
  probeModels: (baseUrl?: string, apiKey?: string) =>
    safeInvoke<string[]>("ai_models_probe", { baseUrl, apiKey }),
  /** 游戏日志一键分析：返回 analysis_id，增量经 `ai://delta/done/error` 事件回传。 */
  analyzeLog: (profileId?: string) =>
    safeInvoke<AiAnalysisHandle>("ai_analyze_log", { profileId }),
  cancelAnalysis: (analysisId: string) =>
    safeInvoke<boolean>("ai_analyze_cancel", { analysisId }),
};

// ── MCP server management（ADR-013 S4/S5）──

export const mcp = {
  status: () => safeInvoke<McpStatusView>("mcp_status"),
  setEnabled: (enabled: boolean) =>
    safeInvoke<McpStatusView>("mcp_set_enabled", { enabled }),
  regenerateToken: () => safeInvoke<McpStatusView>("mcp_regenerate_token"),
  toolsList: () => safeInvoke<McpToolView[]>("mcp_tools_list"),
  toolsSet: (tool: string, policy: string | null) =>
    safeInvoke<void>("mcp_tools_set", { tool, policy }),
  disconnect: (sessionId?: string) =>
    safeInvoke<number>("mcp_sessions_disconnect", { sessionId }),
  confirmations: () => safeInvoke<McpConfirmationView[]>("mcp_confirmations_list"),
  resolveConfirmation: (id: string, allow: boolean) =>
    safeInvoke<boolean>("mcp_confirmation_resolve", { id, allow }),
  recentCalls: (limit?: number) =>
    safeInvoke<McpRecentCallView[]>("mcp_recent_calls", { limit }),
};

export const pluginUi = {
  /**
   * 把 iframe 的**原始**消息转发给宿主桥。
   *
   * 传原始字符串而不是解析后的对象：Rust 侧会重新解析并校验协议（大小/形状/版本/方法），
   * 前端只做来源与配额检查，不参与授权。
   */
  invoke: (pluginId: string, request: string) =>
    safeInvoke<BridgeReply>("plugin_ui_invoke", { pluginId, request }),
};
