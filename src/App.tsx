/**
 * Application shell (ui-design 00 §3): TitleBar + Rail navigation + content
 * area + global overlays (task center, log drawer, toasts, confirm host).
 *
 * The window is borderless/hidden at creation; this shell owns the reveal
 * handshake (unchanged from the previous design).
 */
import { ParentComponent, onMount, Show, For } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";
import TitleBar from "./components/TitleBar";
import { ToastContainer } from "./components/Toast";
import { TaskCenterPanel, TaskIndicator } from "./components/TaskCenter";
import { LogDrawer } from "./components/LogDrawer";
import { LaunchBubble } from "./components/LaunchBubble";
import { RmConfirmHost, RmPromptHost } from "./components/rm/overlays";
import { Icon, type IconName } from "./components/icons";
import { initStores } from "./stores/init";
import { launchStore } from "./stores/launch";
import { downloadsStore } from "./stores/downloads";
import { uiStore } from "./stores/ui";
import { initI18n, t } from "./i18n";
import type { TranslationKey } from "./i18n/locales/messages";
import { initTheme } from "./theme";
import { reportFrontendMessage } from "./ipc/api";

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

interface RailItem {
  href: string;
  icon: IconName;
  label: TranslationKey;
}

const RAIL_ITEMS: RailItem[] = [
  { href: "/", icon: "home", label: "nav.home" },
  { href: "/instances", icon: "gamepad", label: "nav.instances" },
  { href: "/download", icon: "download", label: "nav.download" },
  { href: "/accounts", icon: "person", label: "nav.accounts" },
  { href: "/plugins", icon: "extension", label: "nav.plugins" },
  { href: "/settings", icon: "settings", label: "nav.settings" },
];

const App: ParentComponent = (props) => {
  const location = useLocation();
  const navigate = useNavigate();

  onMount(async () => {
    try {
      const win = getCurrentWindow();
      await initI18n();
      await initTheme();
      initStores();
      await win.show();
      await win.setFocus();
      await ensureReachable(win);
    } catch (e) {
      console.warn("App: window API unavailable", e);
      void reportFrontendMessage(`window shell failed to reveal: ${String(e)}`);
    }
  });

  const isActive = (href: string): boolean => {
    if (href === "/") return location.pathname === "/";
    return location.pathname === href || location.pathname.startsWith(`${href}/`);
  };

  const runningCount = () => launchStore.running().size;
  const downloadCount = () => downloadsStore.batches().size;

  return (
    <div class="app-root">
      <TitleBar />
      <div class="app-body">
        <nav class={`rail${uiStore.railCollapsed() ? " is-collapsed" : ""}`}>
          <For each={RAIL_ITEMS}>
            {(item) => (
              <button
                class={`rail-item${isActive(item.href) ? " is-active" : ""}`}
                onClick={() => navigate(item.href)}
                title={t(item.label)}
                aria-label={t(item.label)}
              >
                <Icon name={item.icon} size={20} />
                <span class="rail-item-label">{t(item.label)}</span>
                <Show when={item.href === "/instances" && runningCount() > 0}>
                  <span class="rail-badge">{runningCount()}</span>
                </Show>
                <Show when={item.href === "/download" && downloadCount() > 0}>
                  <span class="rail-badge">{downloadCount()}</span>
                </Show>
              </button>
            )}
          </For>
          <div class="rail-footer">
            <TaskIndicator />
            <button
              class="rail-item rail-collapse"
              onClick={() => uiStore.toggleRail()}
              aria-label={t("nav.collapse")}
              title={t("nav.collapse")}
            >
              <Icon name="chevron-left" size={18} />
              <span class="rail-item-label">{t("nav.collapse")}</span>
            </button>
          </div>
        </nav>
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
