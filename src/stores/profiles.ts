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
    // A forced refresh must not be swallowed by an in-flight stale request
    // (P2-11): chain a fresh one after it settles instead.
    if (inflight) return force ? inflight.then(() => refresh(true)) : inflight;
    if (!force && items() !== undefined && Date.now() - lastFetch < TTL_MS) return;
    setLoading(true);
    inflight = (async () => {
      const res = await profiles.list();
      if (res.ok) {
        // Preserve object references for unchanged instances (perf audit
        // 2.5): a fresh object per refresh re-triggered every effect keyed
        // on `props.profile` identity and re-sent its IPCs.
        setItems((prev) => {
          if (!prev) return res.data;
          const oldById = new Map(prev.map((p) => [p.id, p]));
          return res.data.map((p) => {
            const old = oldById.get(p.id);
            return old && JSON.stringify(old) === JSON.stringify(p) ? old : p;
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
