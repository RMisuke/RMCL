import { Component, createSignal, onMount, onCleanup, Show, For } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { Icon, type IconName } from "./icons";
import { TaskIndicator } from "./TaskCenter";
import { launchStore } from "../stores/launch";
import { downloadsStore } from "../stores/downloads";
import { uiStore } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";

interface DockItem {
  href: string;
  icon: IconName;
  label: TranslationKey;
}

const DOCK_ITEMS: DockItem[] = [
  { href: "/", icon: "home", label: "nav.home" },
  { href: "/instances", icon: "gamepad", label: "nav.instances" },
  { href: "/download", icon: "download", label: "nav.download" },
  { href: "/accounts", icon: "person", label: "nav.accounts" },
  { href: "/plugins", icon: "extension", label: "nav.plugins" },
  { href: "/settings", icon: "settings", label: "nav.settings" },
];

const TitleBar: Component = () => {
  let appWindow: any = null;
  const [maximized, setMaximized] = createSignal(false);
  const location = useLocation();
  const navigate = useNavigate();
  // Registered in the synchronous component scope: an onCleanup after an
  // awaited onMount has no owner and silently never runs (P1-6).
  let unlistenResized: (() => void) | undefined;
  onCleanup(() => unlistenResized?.());

  onMount(async () => {
    try {
      const mod = await import("@tauri-apps/api/window");
      appWindow = mod.getCurrentWindow();
      setMaximized(await appWindow.isMaximized());

      unlistenResized = await appWindow.onResized(async () => {
        try {
          setMaximized(await appWindow.isMaximized());
        } catch {}
      });
    } catch (e) {
      console.warn("TitleBar: window API unavailable", e);
    }
  });

  const minimize = () => { try { appWindow?.minimize(); } catch {} };
  const toggleMax = () => { try { appWindow?.toggleMaximize(); } catch {} };
  // P1-11: closing the launcher while games are still running needs a confirm.
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

  const isActive = (href: string): boolean => {
    if (href === "/") return location.pathname === "/";
    return location.pathname === href || location.pathname.startsWith(`${href}/`);
  };

  const runningCount = () => launchStore.running().size;
  const downloadCount = () => Object.keys(downloadsStore.batches).length;

  // `data-tauri-drag-region="deep"` makes the whole bar draggable (Tauri walks
  // the composed path); the dock/window buttons are clickable elements and stay
  // excluded from dragging.
  return (
    <div class="titlebar" data-tauri-drag-region="deep">
      <div class="titlebar-brand">SNGL</div>

      {/* 顶部 Dock 导航（图标 + 文字），取代左侧栏（ui/home-hero-dock 试验分支） */}
      <nav class="titlebar-dock" role="navigation">
        <For each={DOCK_ITEMS}>
          {(item) => (
            <button
              class={`dock-item${isActive(item.href) ? " is-active" : ""}`}
              onClick={() => navigate(item.href)}
              title={t(item.label)}
            >
              <Icon name={item.icon} size={16} />
              <span class="dock-item-label">{t(item.label)}</span>
              <Show when={item.href === "/instances" && runningCount() > 0}>
                <span class="dock-badge">{runningCount()}</span>
              </Show>
              <Show when={item.href === "/download" && downloadCount() > 0}>
                <span class="dock-badge">{downloadCount()}</span>
              </Show>
            </button>
          )}
        </For>
      </nav>

      <div class="titlebar-right">
        <TaskIndicator compact />
        <div class="titlebar-controls">
          <button class="titlebar-btn" onClick={minimize} title={t("titlebar.minimize")}>
            <svg width="10" height="10" viewBox="0 0 10 10">
              <line
                x1="1" y1="5" x2="9" y2="5"
                stroke="currentColor"
                stroke-width="1.2"
                stroke-linecap="round"
              />
            </svg>
          </button>
          <button class="titlebar-btn" onClick={toggleMax} title={t("titlebar.maximize")}>
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
          <button class="titlebar-btn titlebar-btn-close" onClick={close} title={t("titlebar.close")}>
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
    </div>
  );
};

export default TitleBar;
