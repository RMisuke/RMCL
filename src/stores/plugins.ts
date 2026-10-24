/**
 * Plugins store: plugin list + diagnostics (rail badge data source).
 * Refetched whenever the plugins page mounts (ui-design 00 §5.1).
 */
import { createRoot, createSignal } from "solid-js";
import { plugins } from "../ipc/api";
import type { PluginList } from "../ipc/types";

export const pluginsStore = createRoot(() => {
  const [list, setList] = createSignal<PluginList | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function refresh(): Promise<void> {
    setLoading(true);
    const res = await plugins.list();
    setLoading(false);
    if (res.ok) {
      setList(res.data);
      setError(null);
    } else {
      setError(res.error.message);
    }
  }

  return { list, loading, error, refresh };
});
