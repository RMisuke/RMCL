/**
 * Left floating dock: account avatar on top, vertical page navigation centred
 * in the remaining height, task-centre trigger pinned at the bottom.
 */
import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { Icon, type IconName } from "./icons";
import { AccountDock } from "./AccountDock";
import { TaskIndicator } from "./TaskCenter";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { themeSkin } from "../theme";

interface DockItem {
  href: string;
  icon: IconName;
  label: TranslationKey;
}

const NAV_ITEMS: DockItem[] = [
  { href: "/", icon: "home", label: "nav.home" },
  { href: "/instances", icon: "gamepad", label: "nav.instances" },
  { href: "/download", icon: "download", label: "nav.download" },
  { href: "/plugins", icon: "extension", label: "nav.plugins" },
];

const SETTINGS_ITEM: DockItem = { href: "/settings", icon: "settings", label: "nav.settings" };

export function LeftDock(): JSX.Element {
  const location = useLocation();
  const navigate = useNavigate();

  const isActive = (href: string): boolean => {
    if (href === "/") return location.pathname === "/";
    return location.pathname === href || location.pathname.startsWith(`${href}/`);
  };

  const skin = () => themeSkin();
  const iconUrl = (name: IconName) => skin()?.navigation?.icons?.[name];

  const DockButton = (props: { item: DockItem }) => (
    <button
      class={`left-dock-item${isActive(props.item.href) ? " is-active" : ""}`}
      onClick={() => navigate(props.item.href)}
      title={t(props.item.label)}
      aria-label={t(props.item.label)}
      aria-current={isActive(props.item.href) ? "page" : undefined}
    >
      <Show when={iconUrl(props.item.icon)} fallback={<Icon name={props.item.icon} size={24} />}>
        <img class="left-dock-icon" src={iconUrl(props.item.icon)!} alt="" />
      </Show>
      <span class="left-dock-tip">{t(props.item.label)}</span>
    </button>
  );

  return (
    <aside class="left-dock" aria-label={t("nav.home")}>
      <AccountDock />

      <nav class="left-dock-nav" role="navigation">
        <For each={NAV_ITEMS}>{(item) => <DockButton item={item} />}</For>
        <DockButton item={SETTINGS_ITEM} />
      </nav>

      <div class="left-dock-foot">
        <TaskIndicator compact />
      </div>
    </aside>
  );
}
