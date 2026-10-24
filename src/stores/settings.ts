/**
 * Settings store: global AppConfig + user settings + theme preference.
 * The settings page and every consumer of effective defaults read from here.
 */
import { createRoot, createSignal } from "solid-js";
import { settings, userSettings, dashboard } from "../ipc/api";
import type { AppConfig, UserSettings } from "../ipc/types";

export const settingsStore = createRoot(() => {
  const [config, setConfig] = createSignal<AppConfig | null>(null);
  const [user, setUser] = createSignal<UserSettings | null>(null);
  const [themePreference, setThemePreference] = createSignal<string | null>(null);
  const [loaded, setLoaded] = createSignal(false);

  async function refresh(): Promise<void> {
    const [cfg, usr, pref] = await Promise.all([
      settings.get(),
      userSettings.get(),
      dashboard.getThemePreference(),
    ]);
    if (cfg.ok) setConfig(cfg.data);
    if (usr.ok) setUser(usr.data);
    if (pref.ok) setThemePreference(pref.data);
    setLoaded(true);
  }

  async function update(patch: Parameters<typeof settings.set>[0]): Promise<{ ok: boolean; message?: string }> {
    const res = await settings.set(patch);
    if (res.ok) {
      setConfig(res.data);
      return { ok: true };
    }
    return { ok: false, message: res.error.message };
  }

  async function updateUser(patch: Parameters<typeof userSettings.set>[0]): Promise<void> {
    const res = await userSettings.set(patch);
    if (res.ok) setUser(res.data);
  }

  async function setTheme(theme: string | null): Promise<{ ok: boolean; message?: string }> {
    // Persist via the theme module, which also applies the tokens to the
    // document — saving alone must never leave the shell unthemed.
    const { setThemePreference: applyTheme } = await import("../theme");
    const res = await applyTheme(theme);
    if (res.ok) setThemePreference(theme);
    return res.ok ? { ok: true } : { ok: false, message: res.error };
  }

  return {
    config,
    user,
    themePreference,
    loaded,
    refresh,
    update,
    updateUser,
    setTheme,
  };
});
