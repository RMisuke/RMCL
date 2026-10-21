/**
 * Application shell.
 *
 * The main window is created borderless, transparent and hidden
 * (`decorations: false`, `transparent: true`, `visible: false`), so the shell
 * owns both the window chrome and the reveal handshake:
 *
 * - `.app-root` paints the rounded, clipped window background. The route roots
 *   (`.launcher-root` / `.login-container`) rely on `flex: 1`, which only lays
 *   out correctly as flex children of this column container — without it the
 *   routes collapse and leave transparent gaps around the content.
 * - `<TitleBar />` supplies the drag region and the window controls. The window
 *   is borderless, so without it the window cannot be moved or closed.
 * - `<ToastContainer />` renders IPC errors; it was previously never mounted,
 *   so every `showToast()` call was invisible to the user.
 * - On mount the window is revealed once the shell has laid out (avoids a white
 *   flash) and checked against the connected monitors, because
 *   `tauri-plugin-window-state` may restore a stale position.
 */
import { ParentComponent, onMount } from "solid-js";
import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";
import TitleBar from "./components/TitleBar";
import { ToastContainer } from "./components/Toast";
import { initI18n } from "./i18n";
import { initTheme } from "./theme";

type AppWindow = Awaited<ReturnType<typeof getCurrentWindow>>;

/** Visible strip required for the title bar to stay grabbable. */
const MIN_VISIBLE_W = 240;
const MIN_VISIBLE_H = 44;

/**
 * Re-centers the window when the restored geometry leaves the title bar
 * off-screen (e.g. state saved while attached to a different display).
 */
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

  if (!reachable) {
    await win.center();
  }
}

const App: ParentComponent = (props) => {
  onMount(async () => {
    try {
      const win = getCurrentWindow();
      // Resolve the UI language before revealing the window, so the first paint
      // is already in the user's language instead of flashing the default.
      await initI18n();
      // Theme tokens must be applied before the first paint to avoid a flash.
      await initTheme();
      // The window is created hidden, so the frontend reveals it once the shell
      // has rendered (`ready-to-show` pattern).
      await win.show();
      await win.setFocus();
      await ensureReachable(win);
    } catch (e) {
      console.warn("App: window API unavailable", e);
    }
  });

  return (
    <div class="app-root">
      <TitleBar />
      {props.children}
      <ToastContainer />
    </div>
  );
};

export default App;
