/**
 * Application shell (ui/soft-skeuo-dashboard 试验分支): 底部浮动 Dock 导航
 * + content area + global overlays (task center, log drawer, launch bubble,
 * toasts, confirm host) + optional local-video wallpaper. 左侧 Rail 已移除。
 *
 * The window is borderless/hidden at creation; this shell owns the reveal
 * handshake (unchanged from the previous design).
 */
import { ParentComponent, onMount } from "solid-js";
import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";
import TitleBar from "./components/TitleBar";
import { AccountDock } from "./components/AccountDock";
import { LaunchDock } from "./components/LaunchDock";
import { ToastContainer } from "./components/Toast";
import { TaskCenterPanel } from "./components/TaskCenter";
import { LogDrawer } from "./components/LogDrawer";
import { LaunchBubble } from "./components/LaunchBubble";
import { RmConfirmHost, RmPromptHost } from "./components/rm/overlays";
import { initStores } from "./stores/init";
import { initI18n } from "./i18n";
import { initTheme } from "./theme";
import { initAdaptiveTheme } from "./lib/adaptiveTheme";
import { reportFrontendMessage } from "./ipc/api";
import { ThemeBackdrop } from "./components/ThemeSurface";
import { BackgroundVideo } from "./components/BackgroundVideo";

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
  onMount(async () => {
    try {
      const win = getCurrentWindow();
      // Theme and locale both gate the first visible paint; load them in
      // parallel instead of paying two serial IPC round-trips.
      await Promise.all([initI18n(), initTheme()]);
      await initAdaptiveTheme();
      initStores();
      await win.show();
      await win.setFocus();
      await ensureReachable(win);
    } catch (e) {
      console.warn("App: window API unavailable", e);
      void reportFrontendMessage(`window shell failed to reveal: ${String(e)}`);
    }
  });

  return (
    <div class="app-root">
      <ThemeBackdrop />
      <BackgroundVideo />
      <div class="window-drag-strip" data-tauri-drag-region="deep" />
      <TitleBar />
      <AccountDock />
      <LaunchDock />
      <div class="app-body">
        {/* 导航在窗口底部浮动 Dock 中，左侧不再有 Rail */}
        <main class="app-content">{props.children}</main>
      </div>
      <TaskCenterPanel />
      <LogDrawer />
      <LaunchBubble />
      <RmConfirmHost />
      <RmPromptHost />
      <ToastContainer />
    </div>
  );
};

export default App;
