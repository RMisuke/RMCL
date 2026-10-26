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
  /** Non-null when the initial config fetch failed (Settings shows a retry). */
  const [loadError, setLoadError] = createSignal<string | null>(null);

  // ── 动态效果开关（宿主统一发布：主题/插件动画都必须跟随此状态）──
  // 发布形式：<html data-motion="on|off"> + CSS 变量 --rmccc-motion-enabled；
  // 主题类插件/自定义主题的动画一律以该状态为闸门，宿主动效（hero 流动层等）同源。
  // ── Debug 模式（开发排查用）：localStorage 持久化；dev 构建默认开启 ──
  const DEBUG_KEY = "rmccc.debugMode";
  const DEBUG_DEFAULT = import.meta.env.DEV;

  function readDebug(): boolean {
    try {
      const raw = localStorage.getItem(DEBUG_KEY);
      if (raw === null) return DEBUG_DEFAULT;
      return raw === "1";
    } catch {
      return DEBUG_DEFAULT;
    }
  }

  const [debugMode, setDebugMode_] = createSignal(readDebug());

  function setDebugMode(enabled: boolean): void {
    setDebugMode_(enabled);
    try {
      localStorage.setItem(DEBUG_KEY, enabled ? "1" : "0");
    } catch {
      /* 存储失败仅影响持久化 */
    }
  }

  const MOTION_KEY = "rmccc.motionEnabled";

  function readMotion(): boolean {
    try {
      return localStorage.getItem(MOTION_KEY) !== "0";
    } catch {
      return true;
    }
  }

  function applyMotion(enabled: boolean): void {
    document.documentElement.dataset.motion = enabled ? "on" : "off";
    document.documentElement.style.setProperty("--rmccc-motion-enabled", enabled ? "1" : "0");
  }

  const [motionEnabled, setMotionEnabled_] = createSignal(readMotion());
  // Guarded for non-DOM environments (tests, workers): P3-11 — module import
  // must not write to `document`.
  if (typeof document !== "undefined") {
    applyMotion(motionEnabled());
  }

  function setMotionEnabled(enabled: boolean): void {
    setMotionEnabled_(enabled);
    applyMotion(enabled);
    try {
      localStorage.setItem(MOTION_KEY, enabled ? "1" : "0");
    } catch {
      /* 持久化失败仅影响下次启动的默认值 */
    }
  }

  async function refresh(): Promise<void> {
    const [cfg, usr, pref] = await Promise.all([
      settings.get(),
      userSettings.get(),
      dashboard.getThemePreference(),
    ]);
    if (cfg.ok) {
      setConfig(cfg.data);
      setLoadError(null);
    } else {
      setLoadError(cfg.error.message);
    }
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
    loadError,
    motionEnabled,
    debugMode,
    setDebugMode,
    setMotionEnabled,
    refresh,
    update,
    updateUser,
    setTheme,
  };
});
