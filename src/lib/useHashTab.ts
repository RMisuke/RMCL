/**
 * Tab state derived from the URL hash for the hash-tab pages (download /
 * instance detail / settings).
 *
 * All tab changes converge on one signal: side-tab clicks, in-page jumps
 * (wizard Step5 → mods, overview shortcuts) and external hash edits all stay
 * in sync. The hash carries only the tab id — query params live on the path
 * (P0-1) and are preserved by `history.replaceState`.
 */
import { createSignal, onCleanup } from "solid-js";

export function useHashTab(
  fallback: string,
  known: readonly string[]
): [() => string, (tab: string) => void] {
  const read = () => {
    const raw = window.location.hash.replace(/^#\/?/, "").split("?")[0];
    return known.includes(raw) ? raw : fallback;
  };
  const [tab, setTab] = createSignal(read());
  // External hash edits (navigate() from other pages) update the signal.
  const onHashChange = () => setTab(read());
  window.addEventListener("hashchange", onHashChange);
  onCleanup(() => window.removeEventListener("hashchange", onHashChange));
  const select = (next: string) => {
    if (next === tab()) return;
    history.replaceState(null, "", `#${next}`);
    setTab(next);
  };
  return [tab, select];
}
