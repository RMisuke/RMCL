/**
 * Home / Dashboard (ui-design 02): quick-launch card (five-state machine),
 * built-in cards (account, quick version, downloads), plugin cards via the
 * isolated bridge, and the layout edit mode (width cycle / hide / reset).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, Switch, Match, ErrorBoundary } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmProgress, RmTag } from "../components/rm/controls";
import { Icon } from "../components/icons";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmEmptyState, RmSkeleton } from "../components/rm/surfaces";
import { PluginFrame } from "../plugin-ui/PluginFrame";
import { AccountAvatar } from "../components/AccountAvatar";
import { dashboard, versions, downloads as downloadsApi, external } from "../ipc/api";
import type { DashboardLayout, WidgetInstance, WidgetDescriptor, VersionInfo, GridSize } from "../ipc/types";
import { profilesStore } from "../stores/profiles";
import { authStore } from "../stores/auth";
import { launchStore } from "../stores/launch";
import { downloadsStore } from "../stores/downloads";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { t } from "../i18n";
import { relativeTime, formatBytes, groupDigits } from "../lib/format";

const WIDGET_WIDTHS: number[] = [2, 3, 4, 6, 8, 12];

export default function Home(): JSX.Element {
  const navigate = useNavigate();
  const [layout, setLayout] = createSignal<DashboardLayout | null>(null);
  const [layoutError, setLayoutError] = createSignal<string | null>(null);
  const [editing, setEditing] = createSignal(false);
  const [hiddenWidgets, setHiddenWidgets] = createSignal<WidgetInstance[]>([]);
  const [addDrawerOpen, setAddDrawerOpen] = createSignal(false);
  const [catalog, setCatalog] = createSignal<WidgetDescriptor[]>([]);

  onMount(async () => {
    void profilesStore.refresh();
    void authStore.refresh();
    const res = await dashboard.getLayout();
    if (res.ok) setLayout(res.data);
    else setLayoutError(res.error.message);
    const widgets = await dashboard.listWidgets();
    if (widgets.ok) setCatalog(widgets.data);
  });

  const profiles = () => profilesStore.items() ?? [];
  const session = () => authStore.session();

  async function saveLayout(next: DashboardLayout): Promise<void> {
    const res = await dashboard.saveLayout(next);
    if (res.ok) {
      setLayout(res.data.layout);
      const corrections = res.data.corrections.length;
      if (corrections > 0) toast({ message: t("home.layoutCorrected", { n: corrections }), tone: "info" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  function cycleWidth(widget: WidgetInstance): void {
    const current = widget.size.w;
    const index = WIDGET_WIDTHS.indexOf(current);
    const next = WIDGET_WIDTHS[(index + 1) % WIDGET_WIDTHS.length] ?? current;
    updateWidget(widget, { w: next });
  }

  function hideWidget(widget: WidgetInstance): void {
    setHiddenWidgets((list) => [...list, widget]);
    setLayout((current) =>
      current ? { ...current, widgets: current.widgets.filter((w) => w.instanceId !== widget.instanceId) } : current
    );
  }

  function updateWidget(widget: WidgetInstance, patch: Partial<GridSize>): void {
    setLayout((current) => {
      if (!current) return current;
      const widgets = current.widgets.map((w) =>
        w.instanceId === widget.instanceId ? { ...w, size: { ...w.size, ...patch } } : w
      );
      if (editing()) void dashboard.saveLayout({ ...current, widgets });
      return { ...current, widgets };
    });
  }

  function addWidget(widgetId: string): void {
    setLayout((current) => {
      if (!current) return current;
      const descriptor = catalog().find((w) => w.widgetId === widgetId);
      if (!descriptor) return current;
      const instance: WidgetInstance = {
        instanceId: `${widgetId}-${Date.now()}`,
        pluginId: descriptor.pluginId,
        widgetId: descriptor.widgetId,
        enabled: true,
        position: { x: 0, y: 999 },
        size: { w: descriptor.defaultSize.w, h: descriptor.defaultSize.h },
        breakpointOverrides: { medium: null, compact: null },
        configuration: {},
      };
      const widgets = [...current.widgets, instance];
      if (editing()) void dashboard.saveLayout({ ...current, widgets });
      return { ...current, widgets };
    });
    setAddDrawerOpen(false);
  }

  function restoreHidden(): void {
    const toRestore = hiddenWidgets();
    if (toRestore.length === 0) return;
    setLayout((current) => {
      if (!current) return current;
      const widgets = [...current.widgets, ...toRestore];
      void dashboard.saveLayout({ ...current, widgets });
      return { ...current, widgets };
    });
    setHiddenWidgets([]);
  }

  async function resetLayout(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("home.resetLayout.title"),
      body: t("home.resetLayout.body"),
      confirmLabel: t("home.resetLayout.confirm"),
    });
    if (!allowed) return;
    const res = await dashboard.resetLayout();
    if (res.ok) {
      setLayout(res.data);
      setHiddenWidgets([]);
    }
  }

  return (
    <div class="page">
      <PageHeader
        title={t("nav.home")}
        actions={
          <div class="btn-row">
            <Show when={editing()}>
              <RmButton size="sm" icon="plus" onClick={() => setAddDrawerOpen(true)}>
                {t("home.addWidget")}
              </RmButton>
              <RmMenu
                align="right"
                items={[
                  { label: t("home.restoreHidden", { n: hiddenWidgets().length }), onSelect: () => restoreHidden(), disabled: hiddenWidgets().length === 0 },
                  { label: t("home.resetLayout.title"), danger: true, onSelect: () => void resetLayout() },
                ]}
              >
                <RmIconButton icon="more-horizontal" label={t("common.more")} />
              </RmMenu>
            </Show>
            <RmButton
              size="sm"
              icon={editing() ? "check" : "edit"}
              variant={editing() ? "primary" : "secondary"}
              onClick={() => {
                if (editing() && layout()) void saveLayout(layout()!);
                setEditing(!editing());
              }}
            >
              {editing() ? t("home.doneEditing") : t("home.editLayout")}
            </RmButton>
          </div>
        }
      />

      <div class="page-body">
        <Show when={layoutError()} fallback={<Show when={!layout()}><RmSkeleton cards /></Show>}>
          <RmEmptyState icon="grid" title={t("home.layoutError")} hint={layoutError()!} />
        </Show>

        <Show when={layout()}>
          {(current) => (
            <div
              class="dashboard-grid"
              style={{ "--grid-cols": String(current().grid.columns) }}
            >
              <For each={current().widgets}>
                {(widget) => (
                  <div
                    class={`dash-widget${editing() ? " is-editing" : ""}${widget.enabled ? "" : " is-disabled-widget"}`}
                    style={{
                      "grid-column": `${widget.position.x + 1} / span ${Math.min(widget.size.w, 12)}`,
                      "grid-row": `${widget.position.y + 1} / span ${widget.size.h}`,
                    }}
                  >
                    <ErrorBoundary
                      fallback={(err, reset) => (
                        <div class="dash-widget-error">
                          <RmEmptyState icon="alert" title={t("home.card.broken")} hint={String(err)} />
                          <div class="btn-row">
                            <RmButton size="sm" onClick={reset}>{t("common.retry")}</RmButton>
                            <RmButton size="sm" variant="danger" onClick={() => hideWidget(widget)}>
                              {t("home.card.hideBroken")}
                            </RmButton>
                          </div>
                        </div>
                      )}
                    >
                      {renderWidget(widget, editing(), catalog(), {
                        cycleWidth: () => cycleWidth(widget),
                        hide: () => hideWidget(widget),
                      })}
                    </ErrorBoundary>
                  </div>
                )}
              </For>
            </div>
          )}
        </Show>
      </div>

      <RmDialog
        open={addDrawerOpen()}
        title={t("home.addWidget")}
        onClose={() => setAddDrawerOpen(false)}
      >
        <div class="add-widget-list">
          <For each={catalog()}>
            {(widget) => (
              <button class="rm-row is-clickable" onClick={() => addWidget(widget.widgetId)}>
                <div class="rm-row-main">
                  <div class="rm-row-title">{widget.displayName}</div>
                  <div class="rm-row-subtitle">
                    {widget.pluginId} · {widget.defaultSize.w}×{widget.defaultSize.h}
                  </div>
                </div>
              </button>
            )}
          </For>
        </div>
      </RmDialog>
    </div>
  );
}



function renderWidget(
  widget: WidgetInstance,
  editing: boolean,
  catalog: WidgetDescriptor[],
  controls: { cycleWidth: () => void; hide: () => void }
): JSX.Element {
  const descriptor = catalog.find(
    (d) => d.pluginId === widget.pluginId && d.widgetId === widget.widgetId
  );
  const editBar = (
    <div class="dash-edit-bar">
      <span class="dash-edit-name">{descriptor?.displayName ?? widget.widgetId}</span>
      <div class="btn-row">
        <RmIconButton icon="edit" label={t("home.cycleWidth")} onClick={controls.cycleWidth} />
        <RmIconButton icon="x" label={t("common.hide")} onClick={controls.hide} />
      </div>
    </div>
  );
  if (widget.pluginId !== "org.rmccc.core-ui") {
    const uiUrl =
      descriptor?.uiUrl ?? `rmcccplugin://${widget.pluginId}/ui/index.html?card=${widget.widgetId}`;
    return (
      <div class="dash-card dash-card-plugin">
        {editing ? editBar : null}
        <div class="dash-plugin-frame">
          <PluginFrame pluginId={widget.pluginId} cardId={widget.widgetId} title={descriptor?.displayName ?? widget.widgetId} uiUrl={uiUrl} />
        </div>
      </div>
    );
  }
  return (
    <div class="dash-card">
      {editing ? editBar : null}
      <div class="dash-card-content">
        <CoreWidget widgetId={widget.widgetId} size={widget.size} />
      </div>
    </div>
  );
}

// ── Built-in widgets (org.rmccc.core-ui) ──

function CoreWidget(props: { widgetId: string; size: GridSize }): JSX.Element {
  return (
    <Switch fallback={<div class="dash-unknown-widget">{props.widgetId}</div>}>
      <Match when={props.widgetId === "instance-launcher"}>
        <QuickLauncher compact={props.size.h <= 2} />
      </Match>
      <Match when={props.widgetId === "account-card"}>
        <AccountCard />
      </Match>
      <Match when={props.widgetId === "version-browser"}>
        <QuickVersions />
      </Match>
      <Match when={props.widgetId === "downloads-card"}>
        <DownloadsCard />
      </Match>
    </Switch>
  );
}

/** Five-state quick launch (ui-design 02 §3.1). */
function QuickLauncher(props: { compact: boolean }): JSX.Element {
  const navigate = useNavigate();
  const profiles = createMemo(() => profilesStore.items() ?? []);
  const session = () => authStore.session();
  const selected = createMemo(() => profilesStore.selected() ?? null);

  createEffect(() => {
    // Default selection: most recently played.
    if (!profilesStore.selectedId() && profiles().length > 0) {
      const latest = [...profiles()].sort(
        (a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? "")
      )[0];
      if (latest) profilesStore.setSelectedId(latest.id);
    }
  });

  const running = (id: string) => launchStore.isRunning(id);
  const starting = (id: string) => launchStore.isStarting(id);

  async function launchCurrent(): Promise<void> {
    const profile = selected();
    if (!profile) return;
    if (!session()) {
      // Not logged in: offer the offline path (02 §3.1) instead of blocking.
      // window.prompt is unsupported in WKWebView — use the prompt dialog host.
      const username = await promptText({
        title: t("accounts.offline.title"),
        label: t("home.offlinePrompt"),
        placeholder: t("accounts.offline.name"),
      });
      if (!username) {
        navigate("/accounts");
        return;
      }
      const res = await authStore.startOfflineLogin(username);
      if (!res.ok) {
        toast({ message: res.error.message, tone: "error" });
        return;
      }
    }
    const result = await launchStore.start(profile.id, profile.name);
    if (!result.ok) toast({ message: result.message ?? "", tone: "error" });
  }

  const recent = createMemo(() =>
    [...profiles()]
      .sort((a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? ""))
      .slice(0, 3)
  );

  return (
    <div class="quick-launch">
      <Switch>
        <Match when={profiles().length === 0}>
          <div class="quick-launch-body is-guide">
            <RmButton size="lg" variant="primary" icon="download" onClick={() => navigate("/download#install")}>
              {t("home.noInstance.cta")}
            </RmButton>
            <p class="quick-launch-hint">{t("home.noInstance.hint")}</p>
          </div>
        </Match>
        <Match when={running(selected()?.id ?? "")}>
          <div class="quick-launch-body">
            <RmButton size="lg" variant="danger" icon="square" onClick={() => void launchStore.kill(selected()!.id)}>
              {t("home.stop")}
            </RmButton>
            <RmButton size="lg" icon="terminal" onClick={() => uiStore.openLogDrawer(selected()?.id ?? null)}>
              {t("home.viewLog")}
            </RmButton>
            <RmButton size="lg" icon="sliders" onClick={() => navigate(`/instances/${selected()!.id}#settings`)}>
              {t("home.instanceSettings")}
            </RmButton>
          </div>
        </Match>
        <Match when={true}>
          <div class="quick-launch-body">
            <div class="quick-launch-main">
              <RmButton
                size="lg"
                variant="primary"
                icon="play"
                loading={starting(selected()?.id ?? "")}
                onClick={() => void launchCurrent()}
              >
                {starting(selected()?.id ?? "") ? t("home.preparing") : t("home.launch")}
              </RmButton>
              <Show when={selected()}>
                {(profile) => (
                  <RmIconButton
                    icon="sliders"
                    label={t("home.instanceSettings")}
                    title={t("home.instanceSettings")}
                    onClick={() => {
                      // 不读 Show 访问器（detail 变化后会抛 stale read），走 store 取当前选择。
                      const id = profilesStore.selectedId() ?? profile().id;
                      if (id) navigate(`/instances/${id}#settings`);
                    }}
                  />
                )}
              </Show>
              <RmMenu
                align="right"
                items={[
                  ...profiles()
                    .sort((a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? ""))
                    .map((p) => ({
                      label: `${p.name} · ${p.game_version}`,
                      onSelect: () => profilesStore.setSelectedId(p.id),
                    })),
                  ...(selected()
                    ? [{ label: t("home.instanceSettings"), icon: "sliders" as const, onSelect: () => navigate(`/instances/${selected()!.id}#settings`) }]
                    : []),
                ]}
              >
                <button class="rm-btn rm-btn-secondary rm-btn-md">
                  {selected()?.name ?? t("home.pickInstance")}
                  <IconChevronDown />
                </button>
              </RmMenu>
            </div>
            <Show when={!props.compact}>
              <div class="quick-launch-meta">
                {selected()?.game_version}
                <Show when={selected()?.mod_loader}>
                  {" · "}
                  {selected()!.mod_loader!.loader_type}
                  {" "}
                  {selected()!.mod_loader!.loader_version}
                </Show>
              </div>
              <div class="quick-launch-recent">
                <For each={recent()}>
                  {(profile) => (
                    <button
                      class="quick-launch-recent-row"
                      onClick={() => profilesStore.setSelectedId(profile.id)}
                    >
                      <span class="quick-launch-recent-name">{profile.name}</span>
                      <span class="quick-launch-recent-time">
                        {profile.last_played
                          ? relativeTime(profile.last_played)
                          : t("instances.neverPlayed")}
                      </span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </Match>
      </Switch>
    </div>
  );
}

function IconChevronDown(): JSX.Element {
  return <Icon name="chevron-down" size={14} />;
}

function AccountCard(): JSX.Element {
  const navigate = useNavigate();
  const session = () => authStore.session();
  // Per-account last launch (launches are recorded against the active account).
  const lastPlayedForAccount = () =>
    session() ? authStore.lastPlayedFor(session()!.account_id) : null;

  async function switchAccount(accountId: string): Promise<void> {
    const res = await authStore.switchAccount(accountId);
    toast({
      message: res.ok ? t("accounts.switched") : res.error.message,
      tone: res.ok ? "success" : "error",
    });
  }

  return (
    <div class="account-card">
      <Show
        when={session()}
        fallback={
          <div class="account-card-empty">
            <p>{t("home.account.signedOut")}</p>
            <RmButton size="sm" icon="plus" onClick={() => navigate("/accounts")}>
              {t("home.account.add")}
            </RmButton>
          </div>
        }
      >
        {(current) => (
          <div class="account-card-body">
            {/* Mojang 正版方形头像（session server 皮肤裁剪，加载失败回退首字母） */}
            <Show
              when={current().provider === "microsoft"}
              fallback={<AccountAvatar name={current().username} provider={current().provider} />}
            >
              <AccountAvatar uuid={current().uuid} name={current().username} provider={current().provider} />
            </Show>
            <div class="account-card-info">
              <div class="account-card-name">{current().username}</div>
              <RmTag tone="accent">{current().provider}</RmTag>
              <div class="account-card-meta">
                {t("home.account.lastPlayed", {
                  time: lastPlayedForAccount() ? relativeTime(lastPlayedForAccount()) : t("instances.neverPlayed"),
                })}
              </div>
            </div>
            <div class="account-card-actions">
              <Show when={current().provider === "microsoft"}>
                <RmIconButton
                  icon="file"
                  label={t("home.account.skin")}
                  title={t("home.account.skin")}
                  onClick={() => void external.openUrl("https://www.minecraft.net/zh-hans/msaprofile/mygames/editskin")}
                />
              </Show>
              <RmMenu
                align="right"
                items={[
                  ...authStore
                    .accounts()
                    .filter((a) => a.account_id !== current().account_id)
                    .map((a) => ({
                      label: a.profile_name,
                      icon: "person" as const,
                      onSelect: () => void switchAccount(a.account_id),
                    })),
                  ...(authStore.accounts().length > 1
                    ? []
                    : [{ label: t("accounts.add.title"), icon: "plus" as const, onSelect: () => navigate("/accounts") }]),
                  { label: t("accounts.manage"), icon: "settings" as const, onSelect: () => navigate("/accounts") },
                  { label: t("accounts.logout"), icon: "log-out" as const, danger: true, onSelect: () => void authStore.logout() },
                ]}
              >
                <RmButton size="sm" icon="person">
                  {t("home.account.switch")}
                </RmButton>
              </RmMenu>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

/** Quick-version chips (ui-design 02 §3.3) — full list lives in the wizard. */
function QuickVersions(): JSX.Element {
  const navigate = useNavigate();
  const [versions_, setVersions_] = createSignal<VersionInfo[] | null>(null);
  const [pasted, setPasted] = createSignal("");

  onMount(async () => {
    const res = await versions.list();
    if (res.ok) setVersions_(res.data.filter((v) => v.release_type === "release"));
  });

  const chips = createMemo(() => {
    const list = versions_() ?? [];
    const seen = new Set<string>();
    const out: VersionInfo[] = [];
    for (const version of list) {
      const major = version.id.split(".").slice(0, 2).join(".");
      if (seen.has(major)) continue;
      seen.add(major);
      out.push(version);
      if (out.length >= 4) break;
    }
    return out;
  });

  return (
    <div class="quick-versions">
      <div class="quick-versions-chips">
        <For each={chips()}>
          {(version) => (
            <button
              class="rm-chip"
              onClick={() => navigate(`/download#install?version=${version.id}`)}
            >
              {version.id}
            </button>
          )}
        </For>
        <button class="rm-chip is-accent" onClick={() => navigate("/download#install")}>
          {t("home.allVersions")}
        </button>
      </div>
      <div class="quick-versions-paste">
        <RmInput
          value={pasted()}
          onInput={setPasted}
          placeholder={t("home.pasteVersion")}
          onEnter={() => pasted() && navigate(`/download#install?version=${pasted()}`)}
        />
      </div>
    </div>
  );
}

/** Downloads card (ui-design 02 §3.4). */
function DownloadsCard(): JSX.Element {
  const navigate = useNavigate();
  const batches = createMemo(() => [...downloadsStore.batches().values()]);

  return (
    <div class="downloads-card">
      <Show
        when={batches().length > 0}
        fallback={
          <div class="downloads-card-empty">
            <span>{t("home.downloads.empty")}</span>
            <button class="rm-chip is-accent" onClick={() => navigate("/download#install")}>
              {t("home.downloads.cta")}
            </button>
          </div>
        }
      >
        <For each={batches()}>
          {(batch) => {
            const percent = () =>
              batch.totalBytes > 0 ? (batch.downloadedBytes / batch.totalBytes) * 100 : 0;
            return (
              <div class="downloads-card-row">
                <div class="downloads-card-info">
                  <div class="downloads-card-title">{batch.label || batch.version}</div>
                  <div class="downloads-card-meta">
                    {groupDigits(batch.completedFiles)}/{groupDigits(batch.totalFiles)} ·{" "}
                    {formatBytes(batch.downloadedBytes)}/{formatBytes(batch.totalBytes)}
                  </div>
                </div>
                <RmProgress value={percent()} />
                <RmIconButton
                  icon="x"
                  label={t("tasks.download.cancel")}
                  onClick={() => void downloadsApi.cancel(batch.batchId)}
                />
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
}
