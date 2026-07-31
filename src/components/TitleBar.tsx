import { Component, createSignal, onMount, onCleanup, Show } from "solid-js";

const TitleBar: Component = () => {
  let appWindow: any = null;
  const [maximized, setMaximized] = createSignal(false);

  onMount(async () => {
    try {
      const mod = await import("@tauri-apps/api/window");
      appWindow = mod.getCurrentWindow();
      setMaximized(await appWindow.isMaximized());

      const unlisten = await appWindow.onResized(async () => {
        try {
          setMaximized(await appWindow.isMaximized());
        } catch {}
      });
      onCleanup(unlisten);
    } catch (e) {
      console.warn("TitleBar: window API unavailable", e);
    }
  });

  const minimize = () => { try { appWindow?.minimize(); } catch {} };
  const toggleMax = () => { try { appWindow?.toggleMaximize(); } catch {} };
  const close = () => { try { appWindow?.close(); } catch {} };

  return (
    <div class="titlebar" data-tauri-drag-region>
      <div class="titlebar-brand">RMCCC</div>
      <div class="titlebar-controls">
        <button class="titlebar-btn" onClick={minimize} title="Minimize">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line
              x1="1" y1="5" x2="9" y2="5"
              stroke="currentColor"
              stroke-width="1.2"
              stroke-linecap="round"
            />
          </svg>
        </button>
        <button class="titlebar-btn" onClick={toggleMax} title="Maximize">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <Show when={maximized()}>
              <rect
                x="3" y="0.5" width="6.5" height="6.5"
                rx="0.8"
                fill="none"
                stroke="currentColor"
                stroke-width="1"
              />
              <rect
                x="0.5" y="3" width="6.5" height="6.5"
                rx="0.8"
                fill="var(--surface-card)"
                stroke="currentColor"
                stroke-width="1"
              />
            </Show>
            <Show when={!maximized()}>
              <rect
                x="1.5" y="1.5" width="7" height="7"
                rx="1"
                fill="none"
                stroke="currentColor"
                stroke-width="1.2"
              />
            </Show>
          </svg>
        </button>
        <button class="titlebar-btn titlebar-btn-close" onClick={close} title="Close">
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line
              x1="1.5" y1="1.5" x2="8.5" y2="8.5"
              stroke="currentColor"
              stroke-width="1.2"
              stroke-linecap="round"
            />
            <line
              x1="8.5" y1="1.5" x2="1.5" y2="8.5"
              stroke="currentColor"
              stroke-width="1.2"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>
    </div>
  );
};

export default TitleBar;
