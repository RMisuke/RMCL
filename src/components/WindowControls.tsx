/**
 * Top-right window controls: three flat rounded buttons (minimise, maximise,
 * close). Rendered directly inside `.app-root` so their stacking context stays
 * below log/task panels.
 */
import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { t } from "../i18n";
import { launchStore } from "../stores/launch";
import { uiStore } from "../stores/ui";

const WindowControls: Component = () => {
  let appWindow: any = null;
  const [maximized, setMaximized] = createSignal(false);
  let unlistenResized: (() => void) | undefined;

  onCleanup(() => {
    unlistenResized?.();
    delete document.documentElement.dataset.windowMaximized;
  });

  const syncMaximized = async () => {
    const next = await appWindow.isMaximized();
    setMaximized(next);
    // CSS corners must vanish when the window fills the screen; otherwise
    // the transparent 18px radius leaves four desktop-coloured notches.
    document.documentElement.dataset.windowMaximized = next ? "true" : "false";
  };

  onMount(async () => {
    try {
      const mod = await import("@tauri-apps/api/window");
      appWindow = mod.getCurrentWindow();
      await syncMaximized();
      unlistenResized = await appWindow.onResized(async () => {
        try {
          await syncMaximized();
        } catch {}
      });
    } catch (e) {
      console.warn("WindowControls: window API unavailable", e);
    }
  });

  const minimize = () => { try { appWindow?.minimize(); } catch {} };
  const toggleMax = () => { try { appWindow?.toggleMaximize(); } catch {} };
  const close = async () => {
    try {
      const running = launchStore.running().size;
      if (running > 0) {
        const allowed = await uiStore.confirm({
          title: t("titlebar.closeConfirm.title"),
          body: t("titlebar.closeConfirm.body", { n: running }),
          confirmLabel: t("titlebar.close"),
          danger: true,
        });
        if (!allowed) return;
      }
      await appWindow?.close();
    } catch {}
  };

  return (
    <div class="window-controls">
      <button class="window-control-btn" onClick={minimize} title={t("titlebar.minimize")}>
        <svg width="11" height="11" viewBox="0 0 11 11">
          <line x1="1.5" y1="5.5" x2="9.5" y2="5.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
        </svg>
      </button>
      <button class="window-control-btn" onClick={toggleMax} title={t("titlebar.maximize")}>
        <svg width="11" height="11" viewBox="0 0 11 11">
          <Show when={maximized()}>
            <rect x="3.2" y="0.8" width="6.4" height="6.4" rx="0.9" fill="none" stroke="currentColor" stroke-width="1.1" />
            <rect x="0.8" y="3.2" width="6.4" height="6.4" rx="0.9" fill="var(--rmccc-surface)" stroke="currentColor" stroke-width="1.1" />
          </Show>
          <Show when={!maximized()}>
            <rect x="1.6" y="1.6" width="7.2" height="7.2" rx="1.1" fill="none" stroke="currentColor" stroke-width="1.2" />
          </Show>
        </svg>
      </button>
      <button class="window-control-btn is-close" onClick={close} title={t("titlebar.close")}>
        <svg width="11" height="11" viewBox="0 0 11 11">
          <line x1="1.8" y1="1.8" x2="9.2" y2="9.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
          <line x1="9.2" y1="1.8" x2="1.8" y2="9.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  );
};

export default WindowControls;
