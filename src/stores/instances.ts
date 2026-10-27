/**
 * Instance store: version-backed instances (ADR-016).
 *
 * Items are full `InstanceDetail` rows (settings + effective + paths) because
 * the list page, launcher and settings form all need them; the backend list
 * command deliberately skips the expensive completeness scan.
 *
 * `instances://refreshed` arrives after an install/repair/rename/remove.
 */
import { createRoot, createSignal } from "solid-js";
import { instances } from "../ipc/api";
import type { InstanceDetail } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

const TTL_MS = 30_000;

export const instancesStore = createRoot(() => {
  const [items, setItems] = createSignal<InstanceDetail[] | undefined>(undefined);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [selectedKey, setSelectedKey] = createSignal<string | null>(null);
  let lastFetch = 0;
  let inflight: Promise<void> | null = null;

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
    get,
    selectedKey,
    setSelectedKey,
    /** Selected instance or the most recently played one (home quick launch). */
    selected: () => {
      const list = items() ?? [];
      return list.find((item) => item.summary.key === selectedKey()) ?? null;
    },
    attachEvents,
  };
});
