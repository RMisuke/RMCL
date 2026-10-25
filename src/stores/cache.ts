/**
 * Icon cache store: config/stats mirror + URL helpers.
 *
 * 渲染路径永远先走 `rmcccicon://`（磁盘直出）；`cache_icon_fetch` 只做后台
 * 条件刷新（stale-while-revalidate），界面从不等待网络。
 */
import { createRoot, createSignal } from "solid-js";
import { cache, auth } from "../ipc/api";
import type { IconCacheStats } from "../ipc/types";

export const cacheStore = createRoot(() => {
  const [enabled, setEnabled] = createSignal(true);
  const [stats, setStats] = createSignal<IconCacheStats | null>(null);

  async function refreshStats(): Promise<void> {
    const res = await cache.stats();
    if (res.ok) {
      setStats(res.data);
      setEnabled(res.data.enabled);
    }
  }

  async function setConfig(next: { enabled?: boolean; maxBytes?: number }): Promise<boolean> {
    const res = await cache.setConfig(next.enabled, next.maxBytes);
    if (res.ok) {
      setStats(res.data);
      setEnabled(res.data.enabled);
      return true;
    }
    return false;
  }

  async function clearAll(): Promise<boolean> {
    const res = await cache.clear();
    if (res.ok) {
      setStats(res.data);
      return true;
    }
    return false;
  }

  /** 缓存协议地址（配合 <img onError> 回退直连，兼容开关关闭/未命中）。 */
  function iconSrc(url: string): string {
    return `rmcccicon://k/${encodeURIComponent(url)}`;
  }

  // 60s 去重：同一 URL 的重复刷新合并成一次 IPC。
  const inFlight = new Map<string, number>();
  function refreshIcon(url: string, kind: "avatar" | "modicon"): void {
    if (!url) return;
    const last = inFlight.get(url) ?? 0;
    if (Date.now() - last < 60_000) return;
    inFlight.set(url, Date.now());
    void cache.iconFetch(url, kind).then(() => {
      if (inFlight.get(url) === last) inFlight.delete(url);
    });
  }

  /**
   * 头像重验：问 sessionserver 拿当前皮肤 URL（换皮肤 = URL 变化），
   * 记入 localStorage 供下次启动首帧直读，然后条件刷新缓存文件。
   */
  async function warmAvatar(uuid: string): Promise<void> {
    const undashed = uuid.replace(/-/g, "");
    const res = await auth.skinUrl(uuid);
    const skin = res.ok ? res.data?.skin ?? null : null;
    if (!skin) return;
    try {
      localStorage.setItem(`rmccc.avatarUrl.${undashed}`, skin);
    } catch {
      /* 仅影响下次首帧 */
    }
    refreshIcon(skin, "avatar");
    // 披风一并刷新（3D 预览与缓存共用）。
    const cape = res.ok ? res.data?.cape ?? null : null;
    if (cape) refreshIcon(cape, "avatar");
  }

  return {
    enabled,
    stats,
    refreshStats,
    setConfig,
    clearAll,
    iconSrc,
    refreshIcon,
    warmAvatar,
  };
});
