/**
 * Profiles store: instance list + in-session selected instance (ui-design 00 §5.1).
 * `instances://refreshed` arrives after version-repository rescan — instance
 * loader state may have changed, so we refetch.
 */
import { createRoot, createSignal } from "solid-js";
import { profiles } from "../ipc/api";
import type { Profile } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

const TTL_MS = 30_000;

export const profilesStore = createRoot(() => {
  const [items, setItems] = createSignal<Profile[] | undefined>(undefined);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  let lastFetch = 0;
  let inflight: Promise<void> | null = null;

  async function refresh(force = false): Promise<void> {
    if (inflight) return inflight;
    if (!force && items() !== undefined && Date.now() - lastFetch < TTL_MS) return;
    setLoading(true);
    inflight = (async () => {
      const res = await profiles.list();
      if (res.ok) {
        setItems(res.data);
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

  function upsert(updated: Profile): void {
    setItems((list) =>
      list?.map((p) => (p.id === updated.id ? updated : p)) ?? [updated]
    );
  }

  function removeItem(id: string): void {
    setItems((list) => list?.filter((p) => p.id !== id));
    if (selectedId() === id) setSelectedId(null);
  }

  function attachEvents(): void {
    void listen(EVENTS.INSTANCES_REFRESHED, () => void refresh(true));
  }

  return {
    items,
    error,
    loading,
    refresh,
    upsert,
    removeItem,
    selectedId,
    setSelectedId,
    /** Selected profile or the most recently played one (home quick launch). */
    selected: () => {
      const list = items() ?? [];
      return list.find((p) => p.id === selectedId()) ?? null;
    },
    attachEvents,
  };
});
