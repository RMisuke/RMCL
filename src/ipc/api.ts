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
  ModLoaderType,
  ModpackInspectView,
  ModpackLoaderView,
  ModpackInstallStart,
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
  SystemMemoryInfo,
  MissingFileInfo,
  InstanceSummary,
  InstanceDetail,
  InstanceSettings,
  InstanceSettingsPatch,
  InstancePaths,
  InstanceIcon,
  GameDirectory,
  GameDirectoryKind,
  GameDirectoryOverview,
  DirectoryCandidate,
  DirectoryLayout,
  DirectorySource,
  StorageOverview,
  StorageEntry,
  StorageKind,
  GameDirectoryStorage,
  InstanceStorage,
  CacheCleanReport,
  GlobalGameSettings,
  EffectiveGameSettings,
  IsolationPolicy,
  TaskBoard,
  TaskSnapshot,
  ServerStatus,
  TaskStart,
  TaskKind,
  TaskState,
  TaskStepState,
  TaskDownloadView,
  TaskStepView,
  TaskCapabilities,
  TaskErrorView,
  TaskResultView,
  PluginList,
  PluginDetail,
  InstallReport,
  PluginSummary,
  PluginConfigView,
  PluginConfig,
  CapabilityInfo,
  PluginContributions,
  WidgetDescriptor,
  ThemeInfo,
  ThemeTokens,
  ThemeSkinPayload,
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
  InstanceSettingsPatch as GeneratedInstanceSettingsPatch,
  AuthSession,
  AccountMetadata,
  ModVersion,
  ModInfo,
  ModSearchResult,
  ModpackInspectView,
  ModpackInstallStart,
  TaskBoard,
  TaskSnapshot,
  ServerStatus,
  TaskStart,
  TaskKind,
  TaskState,
  TaskStepState,
  TaskDownloadView,
  TaskStepView,
  TaskCapabilities,
  TaskErrorView,
  TaskResultView,
  InstanceSummary,
  InstanceDetail,
  InstanceSettingsPatch,
  InstancePaths,
  InstanceIcon,
  GameDirectory,
  GameDirectoryKind,
  GameDirectoryOverview,
  DirectoryCandidate,
  DirectoryLayout,
  DirectorySource,
  StorageOverview,
  StorageEntry,
  StorageKind,
  GameDirectoryStorage,
  InstanceStorage,
  CacheCleanReport,
  GlobalGameSettings,
  EffectiveGameSettings,
  IsolationPolicy,
  VersionInfo,
  AppConfig,
  UserSettings,
  UserSettingsUpdate,
  LocaleInfo,
  IpcError,
  JavaInfo,
  SystemMemoryInfo,
  MissingFileInfo,
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
  WidgetDescriptor,
  ThemeInfo,
  ThemeTokens,
  ThemeSkinPayload,
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

import type { InstanceFolder } from "../lib/instanceFolders";

// Input DTOs: partial updates. The fields come from the ts-rs generated types
// (so they can never drift from the Rust structs), wrapped in `Partial` because
// the backend treats an omitted field as "leave unchanged".
export type SettingsUpdate = Partial<GeneratedSettingsUpdate>;
export type InstanceSettingsPatchInput = Partial<GeneratedInstanceSettingsPatch>;

// ── Safe invoke wrapper ──

export async function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<{ ok: true; data: T } | { ok: false; error: IpcError }> {
  try {
    const data = await invoke<T>(cmd, args);
    return { ok: true, data };
  } catch (e) {
    // Commands always resolve Result<T, IpcError>, but Tauri-layer failures
    // (plugin panics, missing handler) reject with plain strings (P3-3):
    // normalize so `error.message` is never undefined.
    if (e && typeof e === "object" && "code" in e && "message" in e) {
      return { ok: false, error: e as IpcError };
    }
    return {
      ok: false,
      error: { code: "unknown", message: String(e), retryable: false },
    };
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

// ── Instances (ADR-016: version-backed instances) ──

export const instances = {
  /** Full details (settings + effective + paths) for the selected directory,
   *  or all registered directories. `key` = `{directory_id}/{instance_id}`. */
  list: (directoryId?: string, all = false, includeHidden = false) =>
    safeInvoke<InstanceDetail[]>("instance_list", { directoryId, all, includeHidden }),
  get: (key: string) => safeInvoke<InstanceDetail>("instance_get", { key }),
  refresh: (directoryId?: string, includeHidden = false) =>
    safeInvoke<InstanceDetail[]>("instance_refresh", { directoryId, includeHidden }),
  updateSettings: (key: string, opts: InstanceSettingsPatchInput) =>
    safeInvoke<InstanceDetail>("instance_update_settings", { key, opts }),
  resolvedSettings: (key: string) =>
    safeInvoke<ResolvedInstanceSettings>("instance_resolved_settings", { key }),
  paths: (key: string) => safeInvoke<InstancePaths>("instance_paths", { key }),
  /** Open one of the instance's well-known folders in Finder/Explorer. */
  reveal: (key: string, folder: InstanceFolder) =>
    safeInvoke<void>("instance_reveal", { key, folder }),
  select: (key: string | null) => safeInvoke<void>("instance_select", { key }),
  rename: (key: string, newId: string) =>
    safeInvoke<InstanceDetail>("instance_rename", { key, newId }),
  duplicate: (key: string, newId: string, copySaves = false) =>
    safeInvoke<InstanceDetail>("instance_duplicate", { key, newId, copySaves }),
  /** Per-instance disk usage for the instance cards (selected dir default). */
  sizes: (directoryId?: string) =>
    safeInvoke<InstanceStorage[]>("instance_sizes", { directoryId }),
  /** `deleteFiles=false` hides the instance and keeps `versions/{id}`. */
  remove: (key: string, deleteFiles = false) =>
    safeInvoke<void>("instance_remove", { key, deleteFiles }),
  /** Register an already-installed version as a first-class instance. */
  import: (directoryId: string, id: string) =>
    safeInvoke<InstanceDetail>("instance_import", { directoryId, id }),
  /** Download + create a new version-backed instance (background). */
  install: (
    directoryId: string,
    mcVersion: string,
    instanceId?: string,
    loaderType?: string,
    loaderVersion?: string
  ) =>
    safeInvoke<void>("instance_install", {
      directoryId,
      mcVersion,
      instanceId,
      loaderType,
      loaderVersion,
    }),
  checkCompletion: (key: string) =>
    safeInvoke<MissingFileInfo[]>("instance_check_completion", { key }),
  repair: (key: string) => safeInvoke<number>("instance_repair", { key }),
  globalSettings: () => safeInvoke<GlobalGameSettings>("instance_global_settings"),
  updateGlobalSettings: (settings: GlobalGameSettings) =>
    safeInvoke<GlobalGameSettings>("instance_update_global_settings", { settings }),
};

export const gameDirectories = {
  list: () => safeInvoke<GameDirectory[]>("game_directory_list"),
  /** Sidebar rows: directory + standard layout + instance count. */
  overview: () => safeInvoke<GameDirectoryOverview[]>("game_directory_overview"),
  selected: () => safeInvoke<GameDirectory>("game_directory_selected"),
  /** Discover HMCL/PCL/official `.minecraft` directories on this machine. */
  scan: () => safeInvoke<DirectoryCandidate[]>("game_directory_scan"),
  /** Reveal the directory in Finder/Explorer. */
  reveal: (id: string) => safeInvoke<void>("game_directory_reveal", { id }),
  add: (path: string, name?: string, external = true) =>
    safeInvoke<GameDirectory>("game_directory_add", { path, name, external }),
  update: (id: string, name?: string, path?: string) =>
    safeInvoke<GameDirectory>("game_directory_update", { id, name, path }),
  remove: (id: string) => safeInvoke<void>("game_directory_remove", { id }),
  select: (id: string) => safeInvoke<GameDirectory>("game_directory_select", { id }),
};

// ── Storage management ──

export const storage = {
  /** Launcher-owned directories + every game directory with per-instance sizes. */
  overview: () => safeInvoke<StorageOverview>("storage_overview"),
  /** Remove only `{data_dir}/cache` (regenerable download/icon cache). */
  cleanCache: () => safeInvoke<CacheCleanReport>("storage_clean_cache"),
};

// ── Versions ──

export const versions = {
  list: () => safeInvoke<VersionInfo[]>("version_list"),
};

// ── Launch ──

export const launch = {
  start: (key: string) => safeInvoke<void>("launch_start", { key }),
  kill: (key: string) => safeInvoke<void>("launch_kill", { key }),
  status: () => safeInvoke<string[]>("launch_status"),
};

// ── Modpacks（整合包，HMCL 兼容）──

export const modpack = {
  /** Parse a local .zip/.mrpack and return the install-wizard metadata. */
  inspect: (path: string) =>
    safeInvoke<ModpackInspectView>("modpack_inspect", { path }),
  /** Native file picker; null on cancel. */
  pickFile: () => safeInvoke<string | null>("modpack_pick_file"),
  /** Schedule an install. Progress arrives through `task://updated`. */
  install: (path: string, directoryId: string, instanceName?: string) =>
    safeInvoke<ModpackInstallStart>("modpack_install", {
      path,
      directoryId,
      instanceName,
    }),
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
  list: (key: string) => safeInvoke<ModList>("mod_list", { key }),
  toggle: (key: string, modFilename: string, enabled: boolean) =>
    safeInvoke<void>("mod_toggle", { key, modFilename, enabled }),
  install: (key: string, version: ModVersion) =>
    safeInvoke<string>("mod_install", { key, version }),
  remove: (key: string, modFilename: string) =>
    safeInvoke<void>("mod_remove", { key, modFilename }),
  pickAndInstall: (key: string) =>
    safeInvoke<string | null>("mod_pick_and_install", { key }),
  /** 检查缺失前置（ADR-013 S3，纯逻辑无 AI）：扫描报告 + dry-run 补装计划。 */
  dependencyScan: (key: string) =>
    safeInvoke<ModsDependencyScanView>("mods_dependency_scan", { key }),
  /** 一键补装：执行前后端会重新扫描（Core 再校验），只装仍缺失项。 */
  applyPlan: (key: string) =>
    safeInvoke<AppliedPlanReport>("mods_apply_plan", { key }),
};

// ── Mod loaders ──

export const modLoader = {
  list: (loaderType: string, gameVersion: string) =>
    safeInvoke<LoaderVersion[]>("modloader_list", { loaderType, gameVersion }),
  install: (key: string, loaderType: string, loaderVersion: string) =>
    safeInvoke<string>("modloader_install", { key, loaderType, loaderVersion }),
};

// ── Downloads ──

// ── Unified task center ──

export const tasks = {
  /** Active tasks + bounded recent history (survives a WebView reload). */
  list: () => safeInvoke<TaskBoard>("task_list"),
  get: (taskId: string) => safeInvoke<TaskSnapshot | null>("task_get", { taskId }),
  log: (taskId: string) => safeInvoke<string[]>("task_log", { taskId }),
  pause: (taskId: string) => safeInvoke<void>("task_pause", { taskId }),
  resume: (taskId: string) => safeInvoke<void>("task_resume", { taskId }),
  cancel: (taskId: string) => safeInvoke<void>("task_cancel", { taskId }),
  clearHistory: () => safeInvoke<void>("task_clear_history"),
};

// ── Home server card（真实 SLP 查询）──

export const servers = {
  /** Query a Minecraft server with the Server List Ping protocol. */
  status: (address: string) =>
    safeInvoke<ServerStatus>("server_status", { address }),
};

// ── Settings ──

export const settings = {
  get: () => safeInvoke<AppConfig>("settings_get"),
  set: (opts: SettingsUpdate) => safeInvoke<AppConfig>("settings_set", { opts }),
  detectJava: () => safeInvoke<JavaInfo[]>("java_detect"),
  systemMemory: () => safeInvoke<SystemMemoryInfo>("system_memory"),
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

// ── 本地壁纸（Appearance）──

export const backgroundMedia = {
  /** Native MP4 picker; null on cancel. */
  pickVideo: () => safeInvoke<string | null>("dialog_pick_background_video"),
  /** Prepare, validate and authorize one native-resolution hardware-safe asset. */
  prepare: (path: string) =>
    safeInvoke<import("./types").BackgroundVideoAsset>("background_video_prepare", { path }),
  /** Native still-image picker; null on cancel. */
  pickImage: () => safeInvoke<string | null>("dialog_pick_background_image"),
  /** Validate the image and grant asset-protocol access; returns canonical path. */
  prepareImage: (path: string) =>
    safeInvoke<string>("background_image_prepare", { path }),
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

// ── Dashboard themes（布局协议已随 HomeDashboard 下线，B3-1）──

export const dashboard = {
  listWidgets: () =>
    safeInvoke<WidgetDescriptor[]>("dashboard_list_widgets"),
  listThemes: () => safeInvoke<ThemeInfo[]>("dashboard_list_themes"),
  getThemeTokens: (key: string) =>
    safeInvoke<ThemeTokens>("dashboard_get_theme_tokens", { key }),
  getThemeSkin: (key: string) =>
    safeInvoke<ThemeSkinPayload>("dashboard_get_theme_skin", { key }),
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
  analyzeLog: (key?: string) =>
    safeInvoke<AiAnalysisHandle>("ai_analyze_log", { key }),
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
