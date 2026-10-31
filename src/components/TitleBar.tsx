import { Component, createSignal, onMount, onCleanup, Show, For } from "solid-js";
import { Portal } from "solid-js/web";
import { useLocation, useNavigate } from "@solidjs/router";
import { Icon, type IconName } from "./icons";
import { TaskIndicator } from "./TaskCenter";
import { launchStore } from "../stores/launch";
import { tasksStore } from "../stores/tasks";
import { uiStore } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { themeSkin } from "../theme";

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
];
const SETTINGS_ITEM: DockItem = { href: "/settings", icon: "settings", label: "nav.settings" };

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
  const downloadCount = () => tasksStore.activeCount();
  const skin = () => themeSkin();
  const iconUrl = (name: IconName) => skin()?.navigation?.icons?.[name];

  /** One dock slot. Every slot has the same box so the glyph stays optically
   *  centred regardless of badges/tiles. */
  const DockLink = (props: { item: DockItem }) => (
    <button
      class={`dock-item${isActive(props.item.href) ? " is-active" : ""}`}
      onClick={() => navigate(props.item.href)}
      title={t(props.item.label)}
      aria-label={t(props.item.label)}
      aria-current={isActive(props.item.href) ? "page" : undefined}
    >
      <Show when={iconUrl(props.item.icon)} fallback={<Icon name={props.item.icon} size={17} />}>
        <img class="dock-icon" src={iconUrl(props.item.icon)!} alt="" />
      </Show>
      <span class="dock-item-label">{t(props.item.label)}</span>
      <Show when={props.item.href === "/instances" && runningCount() > 0}>
        <span class="dock-badge">{runningCount()}</span>
      </Show>
      <Show when={props.item.href === "/download" && downloadCount() > 0}>
        <span class="dock-badge">{downloadCount()}</span>
      </Show>
    </button>
  );

  return (
    <div class="titlebar">
      <div class="titlebar-dock-top">
        <div class="titlebar-brand" aria-hidden="true">SNGL</div>

        {/* 主题 v2：导航两侧装饰（纯图片，不接管指针） */}
        <For each={skin()?.navigation?.ornaments ?? []}>
          {(ornament) => (
            <img
              class={`titlebar-ornament is-${ornament.side}`}
              src={ornament.image}
              style={{ width: `${ornament.width}px`, opacity: ornament.opacity }}
              alt=""
              aria-hidden="true"
            />
          )}
        </For>

        <nav class="titlebar-dock" role="navigation">
          <For each={DOCK_ITEMS}>{(item) => <DockLink item={item} />}</For>
        </nav>
      </div>

      <div class="titlebar-right">
        <TaskIndicator compact />
        <DockLink item={SETTINGS_ITEM} />
        <Portal>
          <div class="titlebar-controls">
            <button class="titlebar-btn" onClick={minimize} title={t("titlebar.minimize")}>
              <svg width="10" height="10" viewBox="0 0 10 10">
                <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
              </svg>
            </button>
            <button class="titlebar-btn" onClick={toggleMax} title={t("titlebar.maximize")}>
              <svg width="10" height="10" viewBox="0 0 10 10">
                <Show when={maximized()}>
                  <rect x="3" y="0.5" width="6.5" height="6.5" rx="0.8" fill="none" stroke="currentColor" stroke-width="1" />
                  <rect x="0.5" y="3" width="6.5" height="6.5" rx="0.8" fill="var(--surface-card)" stroke="currentColor" stroke-width="1" />
                </Show>
                <Show when={!maximized()}>
                  <rect x="1.5" y="1.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" />
                </Show>
              </svg>
            </button>
            <button class="titlebar-btn titlebar-btn-close" onClick={close} title={t("titlebar.close")}>
              <svg width="10" height="10" viewBox="0 0 10 10">
                <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
                <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
              </svg>
            </button>
          </div>
        </Portal>
      </div>
    </div>
  );
};

export default TitleBar;
