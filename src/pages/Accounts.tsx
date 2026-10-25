/**
 * Accounts (ui-design 06): merged login page — add/switch/remove/refresh,
 * device-code dialog, welcome state when no accounts exist. Tokens are never
 * rendered (06 §10 red line).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, onMount } from "solid-js";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmTag } from "../components/rm/controls";
import { RmDialog } from "../components/rm/overlays";
import { RmCard, RmListRow, RmEmptyState, RmAdvancedSection } from "../components/rm/surfaces";
import { external } from "../ipc/api";
import { AccountAvatar } from "../components/AccountAvatar";
import { SkinViewer } from "../components/SkinViewer";
import { authStore } from "../stores/auth";
import { uiStore, toast } from "../stores/ui";
import { t } from "../i18n";

export default function Accounts(): JSX.Element {
  const [offlineOpen, setOfflineOpen] = createSignal(false);
  const [username, setUsername] = createSignal("");
  const [usernameError, setUsernameError] = createSignal<string | null>(null);
  const [msOpen, setMsOpen] = createSignal(false);

  const session = () => authStore.session();
  const accounts = () => authStore.accounts();
  const deviceCode = () => authStore.deviceCode();
  const noAccounts = createMemo(() => accounts().length === 0);

  onMount(() => {
    void authStore.refresh();
  });

  async function startMicrosoft(): Promise<void> {
    setMsOpen(true);
    const res = await authStore.startMicrosoftLogin();
    if (res.ok) {
      setMsOpen(false);
      authStore.clearDeviceCode();
      toast({ message: t("accounts.loggedIn", { name: authStore.session()?.username ?? "" }), tone: "success" });
      replayPendingLaunch();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function startOffline(): Promise<void> {
    const value = username().trim();
    if (!value || value.length > 32) {
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

  function replayPendingLaunch(): void {
    const pending = authStore.consumePendingLaunch();
    if (pending) void import("../ipc/api").then(({ launch }) => launch.start(pending));
  }

  async function remove(accountId: string, name: string): Promise<void> {
    const allowed = await uiStore.confirm({
      title: t("accounts.remove.title"),
      body: t("accounts.remove.body", { name }),
      confirmLabel: t("accounts.remove.confirm"),
      danger: true,
    });
    if (!allowed) return;
    const res = await authStore.removeAccount(accountId);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  async function refreshLogin(): Promise<void> {
    const res = await authStore.refreshSession();
    toast({
      message: res.ok ? t("accounts.refreshed") : res.error.message,
      tone: res.ok ? "success" : "error",
    });
  }

  return (
    <div class="page accounts-page">
      <PageHeader title={t("accounts.title")} />
      <div class="accounts-split">
        <section class="accounts-main tab-stack">
        {/* Welcome state replaces the old forced /login redirect */}
        <Show when={noAccounts()}>
          <RmCard title={t("accounts.welcome.title")}>
            <div class="welcome-actions">
              <RmButton size="lg" icon="shield" loading={authStore.loggingIn()} onClick={() => void startMicrosoft()}>
                {t("accounts.add.microsoft")}
              </RmButton>
              <RmButton size="lg" icon="person" onClick={() => setOfflineOpen(true)}>
                {t("accounts.add.offline")}
              </RmButton>
            </div>
            <div class="muted-line">{t("accounts.welcome.offlineHint")}</div>
          </RmCard>
        </Show>

        <Show when={session()}>
          {(current) => (
            <RmCard title={t("accounts.current")}>
              <div class="account-card-body">
                <AccountAvatar uuid={current().uuid} name={current().username} provider={current().provider} />
                <div class="account-card-info">
                  <div class="account-card-name">{current().username}</div>
                  <RmTag tone="accent">{current().provider}</RmTag>
                </div>
                <RmIconButton icon="refresh" label={t("accounts.refresh")} onClick={() => void refreshLogin()} />
              </div>
            </RmCard>
          )}
        </Show>

        <Show when={accounts().length > 0}>
          <RmCard title={t("accounts.list")}>
            <div class="rm-list">
              <For each={accounts()}>
                {(account) => (
                  <RmListRow
                    icon={<AccountAvatar uuid={account.profile_id} name={account.profile_name} provider={account.type} small />}
                    title={
                      <span>
                        {account.profile_name}
                        <Show when={account.account_id === session()?.account_id}>
                          {" "}
                          <RmTag tone="success">{t("accounts.currentTag")}</RmTag>
                        </Show>
                      </span>
                    }
                    subtitle={account.type}
                    trailing={
                      <div class="btn-row">
                        <Show when={account.account_id !== session()?.account_id}>
                          <RmButton
                            size="sm"
                            onClick={() =>
                              void authStore.switchAccount(account.account_id).then((res) => {
                                if (res.ok) toast({ message: t("accounts.switched"), tone: "success" });
                                else toast({ message: res.error.message, tone: "error" });
                              })
                            }
                          >
                            {t("accounts.setActive")}
                          </RmButton>
                        </Show>
                        <RmIconButton
                          icon="trash"
                          label={t("accounts.remove.title")}
                          danger
                          onClick={() => void remove(account.account_id, account.profile_name)}
                        />
                      </div>
                    }
                  />
                )}
              </For>
            </div>
          </RmCard>
        </Show>

        <RmCard title={t("accounts.add.title")}>
          <div class="btn-row">
            <RmButton icon="shield" loading={authStore.loggingIn()} onClick={() => void startMicrosoft()}>
              {t("accounts.add.microsoft")}
            </RmButton>
            <RmButton icon="person" onClick={() => setOfflineOpen(true)}>
              {t("accounts.add.offline")}
            </RmButton>
          </div>
        </RmCard>

        <RmAdvancedSection title={t("accounts.securityNoteTitle")}>
          <div class="muted-line">{t("accounts.securityNote")}</div>
        </RmAdvancedSection>

      {/* Device-code dialog */}
      <RmDialog
        open={msOpen()}
        title={t("accounts.ms.title")}
        width={480}
        onClose={() => {
          setMsOpen(false);
          authStore.clearDeviceCode();
        }}
      >
        <Show
          when={deviceCode()}
          fallback={<div class="muted-line">{t("accounts.ms.waiting")}</div>}
        >
          {(code) => (
            <div class="device-code">
              <div class="muted-line">1. {t("accounts.ms.step1")}</div>
              <RmButton icon="external" onClick={() => void external.openUrl(code().verificationUri)}>
                {t("accounts.ms.openLink")}
              </RmButton>
              <div class="muted-line">2. {t("accounts.ms.step2")}</div>
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

      {/* Offline dialog */}
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
        </section>

        {/* 右侧：当前账号皮肤的 3D 模型（skinview3d, MIT） */}
        <aside class="accounts-skin">
          <SkinViewer />
        </aside>
      </div>
    </div>
  );
}
