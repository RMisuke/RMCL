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
} from "./types";

import type {
  SettingsUpdate as GeneratedSettingsUpdate,
  ProfileUpdateOpts as GeneratedProfileUpdateOpts,
  AuthSession,
  AccountMetadata,
  Profile,
  ModVersion,
  ModInfo,
  ModSearchResult,
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
  search: (query: string, gameVersion?: string, loader?: string, page?: number, limit?: number) =>
    safeInvoke<ModSearchResult>("mod_search", { query, gameVersion, loader, page, limit }),
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
  /** Effective on-disk locations; the run dir depends on the isolation setting. */
  paths: (id: string) => safeInvoke<InstancePaths>("instance_paths", { id }),
  refresh: () => safeInvoke<number>("instance_refresh"),
  checkCompletion: (id: string) =>
    safeInvoke<MissingFileInfo[]>("instance_check_completion", { id }),
  repair: (id: string) => safeInvoke<void>("instance_repair", { id }),
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
