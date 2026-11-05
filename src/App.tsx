/**
 * Application shell: left floating dock + content area + window controls +
 * global overlays (task center, log drawer, launch bubble, toasts, confirm
 * hosts) + optional local image/video wallpaper. The launch capsule is
 * Home-only; every other page fills the area right of the dock.
 *
 * The window is borderless/hidden at creation; this shell owns the reveal
 * handshake (unchanged from the previous design).
 */
import { ParentComponent, Show, createSignal, onCleanup, onMount } from "solid-js";
import { useLocation } from "@solidjs/router";
import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";
import WindowControls from "./components/WindowControls";
import { LeftDock } from "./components/LeftDock";
import { LaunchDock } from "./components/LaunchDock";
import { ToastContainer } from "./components/Toast";
import { TaskCenterPanel } from "./components/TaskCenter";
import { LogDrawer } from "./components/LogDrawer";
import { LaunchBubble } from "./components/LaunchBubble";
import { RmConfirmHost, RmPromptHost } from "./components/rm/overlays";
import { Onboarding } from "./components/Onboarding";
import { initStores } from "./stores/init";
import { initI18n } from "./i18n";
import { initTheme } from "./theme";
import { initThemeV4 } from "./theme/v4/store";
import { initAdaptiveTheme } from "./lib/adaptiveTheme";
import { initBackgroundLibrary } from "./lib/backgroundLibrary";
import { reportFrontendMessage } from "./ipc/api";
import { BackdropLayer } from "./backdrop/BackdropLayer";
import { DecorLayer } from "./backdrop/DecorLayer";
import { initRenderTier } from "./render/tier";
import { installCompositingGuard } from "./render/compositing";
import { ModpackInstallDialog } from "./components/ModpackInstallDialog";
import { modpacksStore } from "./stores/modpacks";
import { toast } from "./stores/ui";
import { t } from "./i18n";

type AppWindow = Awaited<ReturnType<typeof getCurrentWindow>>;

const MIN_VISIBLE_W = 240;
const MIN_VISIBLE_H = 44;

async function ensureReachable(win: AppWindow): Promise<void> {
  const monitors = await availableMonitors();
  if (monitors.length === 0) return;
  const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
  const reachable = monitors.some((monitor) => {
    const overlapX =
      Math.min(pos.x + size.width, monitor.position.x + monitor.size.width) -
      Math.max(pos.x, monitor.position.x);
    const overlapY =
      Math.min(pos.y + size.height, monitor.position.y + monitor.size.height) -
      Math.max(pos.y, monitor.position.y);
    return (
      overlapX >= Math.min(MIN_VISIBLE_W, size.width) && overlapY >= MIN_VISIBLE_H
    );
  });
  if (!reachable) await win.center();
}

const App: ParentComponent = (props) => {
  const location = useLocation();
  const onHome = () => location.pathname === "/";
  const [dragActive, setDragActive] = createSignal(false);
  let unlistenDragDrop: (() => void) | undefined;
  let disposeTier: (() => void) | undefined;
  let disposeGuard: (() => void) | undefined;
  onCleanup(() => {
    unlistenDragDrop?.();
    disposeTier?.();
    disposeGuard?.();
  });

  /** Native Tauri drag-drop is the only reliable source of absolute paths on
   *  Windows/WebView2. HTML5 `File.path` is non-standard and undefined there. */
  async function registerDragDrop(): Promise<void> {
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setDragActive(true);
          return;
        }
        setDragActive(false);
        if (payload.type !== "drop") return;

        const path = payload.paths[0];
        if (!path) {
          toast({ message: t("modpack.dropUnavailable"), tone: "info" });
          return;
        }
        if (!/\.(zip|mrpack)$/i.test(path)) {
          toast({ message: t("modpack.dropHint"), tone: "info" });
          return;
        }
        void modpacksStore.inspectPath(path);
      });
    } catch (error) {
      console.warn("App: native drag-drop unavailable", error);
    }
  }

  onMount(async () => {
    // Used by the Windows-only shell radius rule; WebView2 reports "Windows"
    // in the UA, so no extra Tauri OS plugin is needed.
    document.documentElement.dataset.platform = /Windows/i.test(navigator.userAgent)
      ? "windows"
      : "other";
    disposeTier = initRenderTier();
    disposeGuard = installCompositingGuard();
    try {
      const win = getCurrentWindow();
      // Theme and locale both gate the first visible paint; load them in
      // parallel instead of paying two serial IPC round-trips.
      await Promise.all([initI18n(), initTheme()]);
      initThemeV4();
      await initAdaptiveTheme();
      initStores();
      void initBackgroundLibrary();
      await win.show();
      await win.setFocus();
      await ensureReachable(win);
      await registerDragDrop();
    } catch (e) {
      console.warn("App: window API unavailable", e);
      void reportFrontendMessage(`window shell failed to reveal: ${String(e)}`);
    }
  });

  return (
    <div class="app-root">
      <BackdropLayer />
      <DecorLayer />
      <div class="window-drag-strip" data-tauri-drag-region="deep" />
      <LeftDock />
      <Show when={onHome()}>
        <LaunchDock />
      </Show>
      <WindowControls />
      <div class="app-body">
        {/* 导航在窗口底部浮动 Dock 中，左侧不再有 Rail */}
        <main class="app-content">{props.children}</main>
      </div>
      <TaskCenterPanel />
      <LogDrawer />
      <LaunchBubble />
      <RmConfirmHost />
      <RmPromptHost />
      <Onboarding />
      <ModpackInstallDialog />
      <Show when={dragActive()}>
        <div class="modpack-drop-overlay" aria-hidden="true">
          <div class="modpack-drop-card">
            <span class="modpack-drop-icon">＋</span>
            <strong>{t("modpack.dropTitle")}</strong>
            <span>{t("modpack.dropHint")}</span>
          </div>
        </div>
      </Show>
      <Show when={modpacksStore.inspecting()}>
        <div class="modpack-drop-overlay" aria-hidden="true">
          <div class="modpack-drop-card">
            <span class="modpack-drop-icon is-spinning">◌</span>
            <strong>{t("modpack.reading")}</strong>
          </div>
        </div>
      </Show>
      <ToastContainer />
    </div>
  );
};

export default App;
