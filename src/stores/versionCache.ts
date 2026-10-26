/**
 * 游戏版本 / ModLoader 版本缓存（localStorage + 内存镜像）。
 *
 * 策略：读路径永远先回缓存（界面秒开），网络刷新在后台完成后静默替换。
 * 刷新时机：
 * - 游戏版本 manifest：启动器启动时一次（initStores）；下载向导打开也走这里。
 * - Loader 版本：用户打开 Modloader 选择界面时（每次都触发后台刷新）。
 * 数据量：游戏 manifest ~920 条 / loader 每项数百条，localStorage 余量充足。
 */
import { createSignal } from "solid-js";
import { versions, modLoader } from "../ipc/api";
import type { VersionInfo, LoaderVersion } from "../ipc/types";

const GAME_KEY = "rmccc.cache.gameVersions";
const LOADER_KEY = "rmccc.cache.loaderVersions";
/** 缓存有效窗口：窗口内不重复打网络。 */
const FRESH_MS = 10 * 60 * 1000;

/** Pure TTL predicate (B5 test seam): a cache entry written at `fetchedAt`
 *  is still fresh at `now`. */
export function isCacheFresh(fetchedAt: number, now: number): boolean {
  return now - fetchedAt < FRESH_MS;
}

interface GameCache {
  fetchedAt: number;
  list: VersionInfo[];
}
/** Loader entry: `at` doubles as the LRU timestamp (N-7). */
interface LoaderCacheEntry {
  at: number;
  list: LoaderVersion[];
}
interface LoaderCache {
  // key: `${loaderType}|${gameVersion}`
  entries: Record<string, LoaderCacheEntry>;
}
/** Loader cache cap: most recent combinations win (N-7). */
const LOADER_MAX_ENTRIES = 32;

let gameCache: GameCache | null = null;
let loaderCache: LoaderCache | null = null;
let gameRefreshing = false;
let gameRefreshWaiters: ((list: VersionInfo[]) => void)[] = [];
/** Last game-manifest refresh failure; cleared on the next success. */
const [gameError, setGameError] = createSignal<string | null>(null);

function readJson<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* 存储满/隐私模式：缓存仅内存生效 */
  }
}

function loaderEntryKey(loader: string, gameVersion: string): string {
  return `${loader}|${gameVersion}`;
}

// ── 游戏版本 ──

export function cachedGameVersions(): VersionInfo[] | null {
  if (gameCache) return gameCache.list;
  const stored = readJson<GameCache>(GAME_KEY);
  if (stored?.list?.length) {
    gameCache = stored;
    return stored.list;
  }
  return null;
}

export function gameVersionsError(): string | null {
  return gameError();
}

/** 后台刷新游戏版本 manifest；并发调用共享同一次请求。 */
export function refreshGameVersions(): Promise<VersionInfo[] | null> {
  if (gameRefreshing) {
    return new Promise((resolve) => gameRefreshWaiters.push(resolve));
  }
  gameRefreshing = true;
  return versions
    .list()
    .then((res) => {
      if (res.ok && res.data.length > 0) {
        gameCache = { fetchedAt: Date.now(), list: res.data };
        writeJson(GAME_KEY, gameCache);
        setGameError(null);
        return res.data;
      }
      if (!res.ok) setGameError(res.error.message);
      return gameCache?.list ?? null;
    })
    .catch(() => {
      setGameError("request failed");
      return gameCache?.list ?? null;
    })
    .finally(() => {
      gameRefreshing = false;
      const list = gameCache?.list ?? null;
      for (const w of gameRefreshWaiters.splice(0)) w(list ?? []);
    });
}

/** 缓存未过期直接用缓存；否则回缓存并后台刷新。返回快照供界面立即渲染。 */
export function gameVersionsWithRefresh(): VersionInfo[] | null {
  const cached = cachedGameVersions();
  const fresh = gameCache !== null && isCacheFresh(gameCache.fetchedAt, Date.now());
  if (!fresh) void refreshGameVersions();
  return cached;
}

// ── Loader 版本 ──

function readLoaderCache(): LoaderCache {
  const stored = readJson<LoaderCache>(LOADER_KEY);
  // Shape guard: entries from the pre-N-7 layout (bare arrays) are discarded.
  if (stored && Object.values(stored.entries ?? {}).every((e) => e && typeof e.at === "number")) {
    return stored;
  }
  return { entries: {} };
}

export function cachedLoaderVersions(loader: string, gameVersion: string): LoaderVersion[] | null {
  if (!loaderCache) loaderCache = readLoaderCache();
  return loaderCache.entries[loaderEntryKey(loader, gameVersion)]?.list ?? null;
}

export function refreshLoaderVersions(loader: string, gameVersion: string): Promise<LoaderVersion[]> {
  return modLoader.list(loader, gameVersion).then((res) => {
    if (!res.ok) return cachedLoaderVersions(loader, gameVersion) ?? [];
    if (!loaderCache) loaderCache = readLoaderCache();
    const key = loaderEntryKey(loader, gameVersion);
    loaderCache.entries[key] = { at: Date.now(), list: res.data };
    // LRU cap: drop the stalest combinations beyond the cap.
    const keys = Object.entries(loaderCache.entries).sort((a, b) => b[1].at - a[1].at);
    for (const [k] of keys.slice(LOADER_MAX_ENTRIES)) delete loaderCache.entries[k];
    writeJson(LOADER_KEY, loaderCache);
    return res.data;
  });
}
