/**
 * Left-dock account entry: a square MC avatar that opens the account switcher.
 *
 * Each row carries its own actions (Microsoft: refresh token / sign out;
 * offline: edit UUID / delete). The indicator dot is green for a valid active
 * Microsoft session, red once the token has expired, grey for inactive or
 * offline accounts. Adding an account opens a centre dialog that forks into the
 * existing Microsoft device-code or offline username flow.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { AccountAvatar } from "./AccountAvatar";
import { Icon } from "./icons";
import { RmButton, RmIconButton, RmInput } from "./rm/controls";
import { RmDialog } from "./rm/overlays";
import { auth as authApi, external } from "../ipc/api";
import type { AccountMetadata } from "../ipc/types";
import { authStore } from "../stores/auth";
import { toast, uiStore } from "../stores/ui";
import { t } from "../i18n";
import { providerLabel } from "../lib/format";

export function AccountDock(): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [addOpen, setAddOpen] = createSignal(false);
  const [msOpen, setMsOpen] = createSignal(false);
  const [offlineOpen, setOfflineOpen] = createSignal(false);
  const [username, setUsername] = createSignal("");
  const [usernameError, setUsernameError] = createSignal<string | null>(null);
  const [uuidAccount, setUuidAccount] = createSignal<AccountMetadata | null>(null);
  const [uuidValue, setUuidValue] = createSignal("");
  const [uuidError, setUuidError] = createSignal<string | null>(null);
  let root: HTMLDivElement | undefined;

  const session = () => authStore.session();
  const accounts = () => authStore.accounts();
  const deviceCode = () => authStore.deviceCode();

  const tokenExpired = createMemo(() => {
    const expires = session()?.expires_at;
    if (!expires) return false;
    const at = Date.parse(expires);
    return Number.isFinite(at) && at <= Date.now();
  });

  const statusOf = (account: AccountMetadata): "online" | "expired" | "offline" | "idle" => {
    if (account.type !== "microsoft") return "offline";
    if (account.account_id !== session()?.account_id) return "idle";
    return tokenExpired() ? "expired" : "online";
  };

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

  function replayPendingLaunch(): void {
    const pending = authStore.consumePendingLaunch();
    if (pending) void import("../ipc/api").then(({ launch }) => launch.start(pending));
  }

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

  async function refreshToken(): Promise<void> {
    const res = await authStore.refreshSession();
    toast({
      message: res.ok ? t("accounts.refreshed") : res.error.message,
      tone: res.ok ? "success" : "error",
    });
  }

  async function requestSignOut(account: AccountMetadata): Promise<void> {
    const offline = account.type !== "microsoft";
    const allowed = await uiStore.confirm({
      title: offline ? t("dock.account.delete.title") : t("dock.account.logout.title"),
      body: offline
        ? t("dock.account.delete.body", { name: account.profile_name })
        : t("dock.account.logout.body", { name: account.profile_name }),
      confirmLabel: offline ? t("dock.account.delete.confirm") : t("dock.account.logout.confirm"),
      danger: true,
    });
    if (!allowed) return;

    const isCurrent = account.account_id === session()?.account_id;
    if (isCurrent && !offline) {
      await authStore.logout();
    } else {
      const res = await authStore.removeAccount(account.account_id);
      if (!res.ok) {
        toast({ message: res.error.message, tone: "error" });
        return;
      }
    }
    setOpen(false);
  }

  function openUuidEditor(account: AccountMetadata): void {
    setUuidAccount(account);
    setUuidValue(account.profile_id ?? "");
    setUuidError(null);
  }

  async function saveUuid(): Promise<void> {
    const account = uuidAccount();
    if (!account) return;
    const value = uuidValue().trim();
    if (
      value &&
      !/^[0-9a-fA-F]{8}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{12}$/.test(value)
    ) {
      setUuidError(t("dock.account.uuid.invalid"));
      return;
    }
    const res = await authApi.setOfflineUuid(account.account_id, value || null);
    if (!res.ok) {
      setUuidError(res.error.message);
      return;
    }
    await authStore.refresh();
    setUuidAccount(null);
    toast({ message: t("dock.account.uuid.saved"), tone: "success" });
  }

  async function startMicrosoft(): Promise<void> {
    setAddOpen(false);
    setMsOpen(true);
    const res = await authStore.startMicrosoftLogin();
    if (res.ok) {
      setMsOpen(false);
      authStore.clearDeviceCode();
      toast({
        message: t("accounts.loggedIn", { name: authStore.session()?.username ?? "" }),
        tone: "success",
      });
      replayPendingLaunch();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function startOffline(): Promise<void> {
    const value = username().trim();
    if (!value || value.length > 16 || !/^[A-Za-z0-9_]+$/.test(value)) {
      setUsernameError(t("accounts.offline.invalid"));
      return;
    }
    const res = await authStore.startOfflineLogin(value);
    if (res.ok) {
      setOfflineOpen(false);
      setUsername("");
      toast({ message: t("accounts.loggedIn", { name: value }), tone: "success" });
      replayPendingLaunch();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  return (
    <div class="left-dock-account" ref={root}>
      <Show when={open()}>
        <div class="account-popover" role="menu">
          <div class="account-popover-head">
            <span>{t("home.account.switch")}</span>
            <span>{accounts().length}</span>
          </div>

          <For each={accounts()}>
            {(account) => {
              const isCurrent = () => account.account_id === session()?.account_id;
              const microsoft = () => account.type === "microsoft";
              return (
                <div
                  class={`account-row${isCurrent() ? " is-active" : ""}`}
                  role="menuitem"
                  tabindex="0"
                  onClick={() => void switchAccount(account.account_id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      void switchAccount(account.account_id);
                    }
                  }}
                >
                  <span class="account-row-avatar">
                    <AccountAvatar
                      uuid={account.profile_id}
                      name={account.profile_name}
                      provider={account.type}
                      small
                    />
                  </span>
                  <span class="account-row-copy">
                    <strong>{account.profile_name}</strong>
                    <small>
                      {providerLabel(account.type)}
                      <Show when={isCurrent()}> · {t("accounts.currentTag")}</Show>
                    </small>
                  </span>
                  <span class="account-row-actions">
                    <Show
                      when={microsoft()}
                      fallback={
                        <button
                          class="account-action"
                          title={t("dock.account.uuid.button")}
                          aria-label={t("dock.account.uuid.button")}
                          onClick={(event) => {
                            event.stopPropagation();
                            openUuidEditor(account);
                          }}
                        >
                          <Icon name="sliders" size={14} />
                        </button>
                      }
                    >
                      <button
                        class="account-action"
                        title={t("accounts.refresh")}
                        aria-label={t("accounts.refresh")}
                        disabled={!isCurrent()}
                        onClick={(event) => {
                          event.stopPropagation();
                          void refreshToken();
                        }}
                      >
                        <Icon name="refresh" size={14} />
                      </button>
                    </Show>
                    <button
                      class="account-action is-danger"
                      title={microsoft() ? t("accounts.logout") : t("dock.account.delete.title")}
                      aria-label={microsoft() ? t("accounts.logout") : t("dock.account.delete.title")}
                      onClick={(event) => {
                        event.stopPropagation();
                        void requestSignOut(account);
                      }}
                    >
                      <Icon name={microsoft() ? "log-out" : "trash"} size={14} />
                    </button>
                    <span class={`account-status is-${statusOf(account)}`} />
                  </span>
                </div>
              );
            }}
          </For>

          <Show when={accounts().length === 0}>
            <div class="account-popover-empty">{t("dock.account.signedOut")}</div>
          </Show>

          <div class="account-popover-sep" />
          <button
            class="account-add-row"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setAddOpen(true);
            }}
          >
            <Icon name="plus" size={15} />
            {t("home.account.add")}
          </button>
          <Show when={session()?.provider === "microsoft"}>
            <button
              class="account-add-row"
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
        </div>
      </Show>

      <button
        class={`account-avatar-trigger${open() ? " is-open" : ""}`}
        aria-haspopup="menu"
        aria-expanded={open()}
        aria-label={t("home.account.switch")}
        title={session()?.username ?? t("home.account.add")}
        onClick={() => setOpen(!open())}
      >
        <Show when={session()} fallback={<Icon name="person" size={19} />}>
          {(current) => (
            <AccountAvatar
              uuid={current().uuid}
              name={current().username}
              provider={current().provider}
            />
          )}
        </Show>
        <span
          class={`account-trigger-dot${session() ? (tokenExpired() ? " is-expired" : " is-online") : ""}`}
        />
      </button>

      {/* Centre dialog: choose the login type */}
      <RmDialog open={addOpen()} title={t("accounts.add.title")} onClose={() => setAddOpen(false)}>
        <div class="add-account-choices">
          <button class="add-account-choice" onClick={() => void startMicrosoft()}>
            <Icon name="shield" size={22} />
            <strong>{t("accounts.add.microsoft")}</strong>
            <small>{t("dock.account.add.microsoftHint")}</small>
          </button>
          <button
            class="add-account-choice"
            onClick={() => {
              setAddOpen(false);
              setOfflineOpen(true);
            }}
          >
            <Icon name="person" size={22} />
            <strong>{t("accounts.add.offline")}</strong>
            <small>{t("dock.account.add.offlineHint")}</small>
          </button>
        </div>
      </RmDialog>

      {/* Microsoft device-code dialog */}
      <RmDialog
        open={msOpen()}
        title={t("accounts.ms.title")}
        width={480}
        onClose={() => {
          setMsOpen(false);
          authStore.clearDeviceCode();
        }}
      >
        <Show when={deviceCode()} fallback={<div class="muted-line">{t("accounts.ms.waiting")}</div>}>
          {(code) => (
            <div class="device-code">
              <div class="muted-line">{t("accounts.ms.step1")}</div>
              <RmButton icon="external" onClick={() => void external.openUrl(code().verificationUri)}>
                {t("accounts.ms.openLink")}
              </RmButton>
              <div class="muted-line">{t("accounts.ms.step2")}</div>
              <div class="device-code-value">
                {code().userCode}
                <RmIconButton
                  icon="copy"
                  label={t("common.copy")}
                  onClick={() => {
                    void navigator.clipboard.writeText(code().userCode);
                    toast({ message: t("accounts.ms.codeCopied"), tone: "success" });
                  }}
                />
              </div>
              <div class="muted-line">{t("accounts.ms.waiting")}</div>
            </div>
          )}
        </Show>
      </RmDialog>

      {/* Offline username dialog */}
      <RmDialog
        open={offlineOpen()}
        title={t("accounts.offline.title")}
        onClose={() => setOfflineOpen(false)}
        footer={
          <>
            <RmButton variant="ghost" onClick={() => setOfflineOpen(false)}>{t("common.cancel")}</RmButton>
            <RmButton variant="primary" onClick={() => void startOffline()}>{t("accounts.offline.create")}</RmButton>
          </>
        }
      >
        <RmInput
          value={username()}
          onInput={(v) => {
            setUsername(v);
            setUsernameError(null);
          }}
          placeholder={t("accounts.offline.name")}
          error={usernameError()}
          onEnter={() => void startOffline()}
        />
      </RmDialog>

      {/* Offline UUID editor */}
      <RmDialog
        open={!!uuidAccount()}
        title={t("dock.account.uuid.title")}
        onClose={() => setUuidAccount(null)}
        footer={
          <>
            <RmButton variant="ghost" onClick={() => setUuidAccount(null)}>{t("common.cancel")}</RmButton>
            <RmButton variant="primary" onClick={() => void saveUuid()}>{t("common.save")}</RmButton>
          </>
        }
      >
        <RmInput
          value={uuidValue()}
          onInput={(v) => {
            setUuidValue(v);
            setUuidError(null);
          }}
          placeholder={t("dock.account.uuid.placeholder")}
          error={uuidError()}
          onEnter={() => void saveUuid()}
        />
        <div class="muted-line account-uuid-hint">{t("dock.account.uuid.hint")}</div>
      </RmDialog>
    </div>
  );
}
