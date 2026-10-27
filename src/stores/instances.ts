/**
 * Instance store: version-backed instances (ADR-016) + game-directory registry.
 *
 * Items are full `InstanceDetail` rows (settings + effective + paths) because
 * the list page, launcher and settings form all need them; the backend list
 * command deliberately skips the expensive completeness scan.
 *
 * The directory list is global state: the instance list, the Home quick-launch
 * card and the "new instance" wizard all operate on the *selected* directory,
 * exactly like HMCL's game-directory drawer and PCL's folder list.
 *
 * `instances://refreshed` arrives after an install/repair/rename/remove.
 */
import { createRoot, createSignal } from "solid-js";
import { instances, gameDirectories } from "../ipc/api";
import type {
  DirectoryCandidate,
  GameDirectory,
  GameDirectoryOverview,
  InstanceDetail,
} from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

const TTL_MS = 30_000;

type DirectoryResult<T> = { ok: true; data: T } | { ok: false; error: { message: string } };

export const instancesStore = createRoot(() => {
  const [items, setItems] = createSignal<InstanceDetail[] | undefined>(undefined);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [selectedKey, setSelectedKey] = createSignal<string | null>(null);

  const [directories, setDirectories] = createSignal<GameDirectoryOverview[] | undefined>(undefined);
  const [directoriesLoading, setDirectoriesLoading] = createSignal(false);
  const [directoriesError, setDirectoriesError] = createSignal<string | null>(null);
  const [directoryBusy, setDirectoryBusy] = createSignal(false);

  let lastFetch = 0;
  let inflight: Promise<void> | null = null;
  let directoriesInflight: Promise<void> | null = null;
  let lastDirectoriesFetch = 0;

  async function refresh(force = false): Promise<void> {
    if (inflight) return force ? inflight.then(() => refresh(true)) : inflight;
    if (!force && items() !== undefined && Date.now() - lastFetch < TTL_MS) return;
    setLoading(true);
    inflight = (async () => {
      const res = await instances.list();
      if (res.ok) {
        // Preserve object identity for unchanged rows (avoids effect storms).
        setItems((prev) => {
          if (!prev) return res.data;
          const old = new Map(prev.map((p) => [p.summary.key, p]));
          return res.data.map((p) => {
            const previous = old.get(p.summary.key);
            return previous && JSON.stringify(previous) === JSON.stringify(p)
              ? previous
              : p;
          });
        });
        setError(null);
        lastFetch = Date.now();
      } else {
        setError(res.error.message);
      }
      setLoading(false);
      inflight = null;
    })();
    return inflight;
  }

  /**
   * Rescan the versions/ folder of the active directory before reloading the
   * list. `instance_list` deliberately does not rescan (it runs on every page
   * load); this is what the refresh button and directory switches use, so
   * versions installed by another launcher show up immediately.
   */
  async function rescan(): Promise<void> {
    const res = await instances.refresh();
    if (!res.ok) {
      setError(res.error.message);
      return;
    }
    await Promise.all([refresh(true), refreshDirectories(true)]);
  }

  /** Refresh the directory sidebar (counts + layout badges). */
  async function refreshDirectories(force = false): Promise<void> {
    if (directoriesInflight) {
      return force ? directoriesInflight.then(() => refreshDirectories(true)) : directoriesInflight;
    }
    if (!force && directories() !== undefined && Date.now() - lastDirectoriesFetch < TTL_MS) {
      return;
    }
    setDirectoriesLoading(true);
    directoriesInflight = (async () => {
      const res = await gameDirectories.overview();
      if (res.ok) {
        setDirectories(res.data);
        setDirectoriesError(null);
        lastDirectoriesFetch = Date.now();
      } else {
        setDirectoriesError(res.error.message);
      }
      setDirectoriesLoading(false);
      directoriesInflight = null;
    })();
    return directoriesInflight;
  }

  /** Selected game directory (backend fallback: first registered). */
  function selectedDirectory(): GameDirectoryOverview | null {
    const list = directories() ?? [];
    return list.find((entry) => entry.is_selected) ?? list[0] ?? null;
  }

  function directoryById(id: string): GameDirectoryOverview | undefined {
    return (directories() ?? []).find((entry) => entry.directory.id === id);
  }

  function markSelectedDirectory(id: string): void {
    setDirectories((prev) =>
      prev?.map((entry) => ({ ...entry, is_selected: entry.directory.id === id }))
    );
  }

  /**
   * Switch the active game directory.
   *
   * Selection is persisted by the backend (like HMCL's `selectedGameDirectory`),
   * the instance list is reloaded, and any selected instance that belongs to the
   * previous directory is dropped so Home can pick a new default.
   */
  async function selectDirectory(id: string): Promise<DirectoryResult<GameDirectory>> {
    if (selectedDirectory()?.directory.id === id) {
      return {
        ok: true,
        data: selectedDirectory()!.directory,
      };
    }
    setDirectoryBusy(true);
    const res = await gameDirectories.select(id);
    if (!res.ok) {
      setDirectoryBusy(false);
      return res;
    }
    markSelectedDirectory(id);
    if (selectedKey() && !selectedKey()!.startsWith(`${id}/`)) {
      setSelectedKey(null);
    }
    // Rescan on switch: another launcher may have added versions while RMCCC
    // was open, and the repository cache would otherwise hide them.
    await instances.refresh(id);
    await Promise.all([refresh(true), refreshDirectories(true)]);
    setDirectoryBusy(false);
    return res;
  }

  async function addDirectory(path: string, name?: string): Promise<DirectoryResult<GameDirectory>> {
    setDirectoryBusy(true);
    const res = await gameDirectories.add(path, name, true);
    setDirectoryBusy(false);
    if (res.ok) await refreshDirectories(true);
    return res;
  }

  async function updateDirectory(
    id: string,
    name?: string,
    path?: string
  ): Promise<DirectoryResult<GameDirectory>> {
    const res = await gameDirectories.update(id, name, path);
    if (res.ok) await Promise.all([refreshDirectories(true), refresh(true)]);
    return res;
  }

  async function removeDirectory(id: string): Promise<DirectoryResult<void>> {
    setDirectoryBusy(true);
    const res = await gameDirectories.remove(id);
    if (res.ok) {
      await refreshDirectories(true);
      await refresh(true);
    }
    setDirectoryBusy(false);
    return res;
  }

  async function scanDirectories(): Promise<DirectoryResult<DirectoryCandidate[]>> {
    return gameDirectories.scan();
  }

  async function revealDirectory(id: string): Promise<DirectoryResult<void>> {
    return gameDirectories.reveal(id);
  }

  function upsert(updated: InstanceDetail): void {
    setItems((list) => {
      const key = updated.summary.key;
      if (!list) return [updated];
      const found = list.some((item) => item.summary.key === key);
      return found
        ? list.map((item) => (item.summary.key === key ? updated : item))
        : [updated, ...list];
    });
  }

  function removeItem(key: string): void {
    setItems((list) => list?.filter((item) => item.summary.key !== key));
    if (selectedKey() === key) setSelectedKey(null);
  }

  function get(key: string): InstanceDetail | undefined {
    return (items() ?? []).find((item) => item.summary.key === key);
  }

  /** Select an instance and persist it (Home quick launch + dashboard card). */
  async function selectInstance(key: string | null): Promise<void> {
    setSelectedKey(key);
    await instances.select(key);
  }

  function attachEvents(): void {
    void listen(EVENTS.INSTANCES_REFRESHED, () => {
      void refresh(true);
      void refreshDirectories(true);
    });
  }

  return {
    items,
    error,
    loading,
    refresh,
    rescan,
    upsert,
    removeItem,
    get,
    selectedKey,
    selectInstance,
    /** Selected instance or the most recently played one (home quick launch). */
    selected: () => {
      const list = items() ?? [];
      return list.find((item) => item.summary.key === selectedKey()) ?? null;
    },
    // ── Game directories ──
    directories,
    directoriesLoading,
    directoriesError,
    directoryBusy,
    refreshDirectories,
    selectedDirectory,
    directoryById,
    selectDirectory,
    addDirectory,
    updateDirectory,
    removeDirectory,
    scanDirectories,
    revealDirectory,
    attachEvents,
  };
});
