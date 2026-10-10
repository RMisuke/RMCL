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
  JavaInfo,
  InstanceInfo,
  DownloadProgressPayload,
} from "./types";

import type {
  AuthSession,
  AccountMetadata,
  Profile,
  ModVersion,
  VersionInfo,
  AppConfig,
  UserSettings,
  UserSettingsUpdate,
  LocaleInfo,
  IpcError,
  JavaInfo,
} from "./types";

// 输入型 DTO：允许部分更新（后端字段为 Option，未提供字段保持原值）
export type SettingsUpdate = Partial<{
  game_root_dir: string | null;
  default_java_path: string | null;
  default_min_memory_mb: number | null;
  default_max_memory_mb: number | null;
  download_concurrency: number | null;
  mirror_url: string | null;
}>;

export type ProfileUpdateOpts = Partial<{
  name: string;
  game_version: string;
  java_path: string;
  jvm_args: string[];
  game_args: string[];
  memory_mb: number;
  icon: string;
}>;

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
  microsoftLogin: () => invoke<AuthSession>("auth_microsoft_login"),
  offlineLogin: (username: string) =>
    invoke<AuthSession>("auth_offline_login", { username }),
  logout: () => invoke<void>("auth_logout"),
  getSession: () => invoke<AuthSession | null>("auth_get_session"),
  refreshToken: () => invoke<AuthSession | null>("auth_refresh_token"),
  listAccounts: () => invoke<AccountMetadata[]>("auth_list_accounts"),
  switchAccount: (accountId: string) =>
    invoke<AuthSession>("auth_switch_account", { accountId }),
  removeAccount: (accountId: string) =>
    invoke<void>("auth_remove_account", { accountId }),
};

// ── Profiles ──

export const profiles = {
  list: () => invoke<Profile[]>("profile_list"),
  create: (name: string, gameVersion: string) =>
    invoke<Profile>("profile_create", { name, gameVersion }),
  update: (id: string, opts: ProfileUpdateOpts) =>
    invoke<Profile>("profile_update", { id, opts }),
  delete: (id: string) => invoke<void>("profile_delete", { id }),
};

// ── Versions ──

export const versions = {
  list: () => invoke<VersionInfo[]>("version_list"),
  install: (version: string, profileId: string) =>
    invoke<void>("version_install", { version, profileId }),
};

// ── Launch ──

export const launch = {
  start: (profileId: string) => invoke<void>("launch_start", { profileId }),
  kill: (profileId: string) => invoke<void>("launch_kill", { profileId }),
  status: () => invoke<string[]>("launch_status"),
};

// ── Mods ──

export const mods = {
  search: (query: string, gameVersion?: string, loader?: string, page?: number, limit?: number) =>
    invoke("mod_search", { query, gameVersion, loader, page, limit }),
  getVersions: (modId: string) =>
    invoke<ModVersion[]>("mod_get_versions", { modId }),
  getDownloadUrl: (versionId: string) =>
    invoke<string>("mod_get_download_url", { versionId }),
  list: (profileId: string) =>
    invoke<string[]>("mod_list", { profileId }),
  toggle: (profileId: string, modFilename: string, enabled: boolean) =>
    invoke<void>("mod_toggle", { profileId, modFilename, enabled }),
  install: (profileId: string, version: ModVersion) =>
    invoke<string>("mod_install", { profileId, version }),
  remove: (profileId: string, modFilename: string) =>
    invoke<void>("mod_remove", { profileId, modFilename }),
};

// ── Downloads ──

export const downloads = {
  cancel: (batchId: string) => invoke<void>("download_cancel", { batchId }),
  pause: (batchId: string) => invoke<void>("download_pause", { batchId }),
  resume: (batchId: string) => invoke<void>("download_resume", { batchId }),
};

// ── Settings ──

export const settings = {
  get: () => invoke<AppConfig>("settings_get"),
  set: (opts: SettingsUpdate) => invoke<AppConfig>("settings_set", { opts }),
  detectJava: () => invoke<JavaInfo[]>("java_detect"),
};

// ── User-level settings（UI 语言等）──

export const userSettings = {
  get: () => invoke<UserSettings>("settings_get_user"),
  /** `locale: ""` = follow the system language; a tag pins it; `null` = unchanged. */
  set: (opts: UserSettingsUpdate) =>
    invoke<UserSettings>("settings_set_user", { opts }),
  supportedLocales: () => invoke<LocaleInfo[]>("settings_supported_locales"),
};

// ── Logs（P3-01：补 log_get/log_clear wrapper）──

export const logs = {
  get: (lines?: number) => invoke<string[]>("log_get", { lines }),
  clear: () => invoke<void>("log_clear"),
};

// ── External ──

export const external = {
  openUrl: (url: string) => invoke<void>("open_external_url", { url }),
};
