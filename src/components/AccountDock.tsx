/**
 * Bottom-left account dock (soft-skeuo dashboard).
 *
 * Shows the signed-in account with a square rounded avatar, provider badge and
 * the launcher's cumulative play time; clicking opens a glass popover with the
 * account list so switching is one click, mirroring the launch dock's version
 * picker.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { AccountAvatar } from "./AccountAvatar";
import { Icon } from "./icons";
import { external } from "../ipc/api";
import { authStore } from "../stores/auth";
import { instancesStore } from "../stores/instances";
import { toast } from "../stores/ui";
import { t } from "../i18n";
import { playtimeLabel } from "../lib/playtime";
import { providerLabel, relativeTime } from "../lib/format";

export function AccountDock(): JSX.Element {
  const navigate = useNavigate();
  const [open, setOpen] = createSignal(false);
  let root: HTMLDivElement | undefined;

  const session = () => authStore.session();
  const accounts = () => authStore.accounts();
  const totalPlaytime = createMemo(() =>
    (instancesStore.items() ?? []).reduce(
      (sum, item) => sum + Number(item.summary.play_time_seconds ?? 0),
      0
    )
  );

  const onDocDown = (event: MouseEvent) => {
    if (open() && root && !root.contains(event.target as Node)) setOpen(false);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && open()) setOpen(false);
  };
  onMount(() => {
    document.addEventListener("mousedown", onDocDown);
    document.addEventListener("keydown", onKeyDown);
  });
  onCleanup(() => {
    document.removeEventListener("mousedown", onDocDown);
    document.removeEventListener("keydown", onKeyDown);
  });

  async function switchAccount(accountId: string): Promise<void> {
    if (accountId === session()?.account_id) {
      setOpen(false);
      return;
    }
    const res = await authStore.switchAccount(accountId);
    toast({
      message: res.ok ? t("accounts.switched") : res.error.message,
      tone: res.ok ? "success" : "error",
    });
    setOpen(false);
  }

  function go(href: string): void {
    setOpen(false);
    navigate(href);
  }

  return (
    <div class="account-dock" ref={root}>
      <Show when={open()}>
        <div class="dock-popover account-popover" role="menu">
          <div class="dock-popover-head">
            <span>{t("home.account.switch")}</span>
            <span>{accounts().length}</span>
          </div>
          <For each={accounts()}>
            {(account) => (
              <button
                class={`dock-account-row${account.account_id === session()?.account_id ? " is-active" : ""}`}
                role="menuitem"
                onClick={() => void switchAccount(account.account_id)}
              >
                <span class="dock-row-avatar">
                  <AccountAvatar
                    uuid={account.profile_id}
                    name={account.profile_name}
                    provider={account.type}
                  />
                </span>
                <span class="dock-row-copy">
                  <strong>{account.profile_name}</strong>
                  <small>
                    {providerLabel(account.type)}
                    <Show when={authStore.lastPlayedFor(account.account_id)}>
                      {" · "}
                      {t("home.account.lastPlayed", {
                        time: relativeTime(authStore.lastPlayedFor(account.account_id)),
                      })}
                    </Show>
                  </small>
                </span>
                <Show when={account.account_id === session()?.account_id}>
                  <Icon name="check" size={15} />
                </Show>
              </button>
            )}
          </For>
          <Show when={accounts().length === 0}>
            <div class="dock-popover-empty">{t("dock.account.signedOut")}</div>
          </Show>
          <div class="dock-popover-sep" />
          <button class="dock-action-row" role="menuitem" onClick={() => go("/accounts")}>
            <Icon name="plus" size={15} />
            {t("home.account.add")}
          </button>
          <button class="dock-action-row" role="menuitem" onClick={() => go("/accounts")}>
            <Icon name="settings" size={15} />
            {t("accounts.manage")}
          </button>
          <Show when={session()?.provider === "microsoft"}>
            <button
              class="dock-action-row"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                void external.openUrl("https://www.minecraft.net/msaprofile/mygames/editskin");
              }}
            >
              <Icon name="external" size={15} />
              {t("home.account.skin")}
            </button>
          </Show>
          <Show when={session()}>
            <button
              class="dock-action-row is-danger"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                void authStore.logout();
              }}
            >
              <Icon name="log-out" size={15} />
              {t("accounts.logout")}
            </button>
          </Show>
        </div>
      </Show>

      <button
        class={`account-dock-trigger${open() ? " is-open" : ""}`}
        aria-haspopup="menu"
        aria-expanded={open()}
        aria-label={t("home.account.switch")}
        onClick={() => setOpen(!open())}
      >
        <Show
          when={session()}
          fallback={
            <>
              <span class="account-dock-avatar is-empty">
                <Icon name="person" size={19} />
              </span>
              <span class="account-dock-copy">
                <span class="account-dock-name">{t("dock.account.signedOut")}</span>
                <span class="account-dock-meta">{t("home.account.add")}</span>
              </span>
            </>
          }
        >
          {(current) => (
            <>
              <span class="account-dock-avatar">
                <AccountAvatar
                  uuid={current().uuid}
                  name={current().username}
                  provider={current().provider}
                />
              </span>
              <span class="account-dock-copy">
                <span class="account-dock-name">{current().username}</span>
                <span class="account-dock-meta">
                  <span class="account-dock-provider">{providerLabel(current().provider)}</span>
                  <span class="account-dock-dot" />
                  <Icon name="clock" size={11} />
                  <span class="account-dock-time">
                    {totalPlaytime() > 0
                      ? t("playtime.cumulative", { time: playtimeLabel(totalPlaytime()) })
                      : t("playtime.none")}
                  </span>
                </span>
              </span>
            </>
          )}
        </Show>
        <Icon
          name={open() ? "chevron-down" : "chevron-up"}
          size={14}
          class="account-dock-chevron"
        />
      </button>
    </div>
  );
}
