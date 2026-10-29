import type { JSX } from "solid-js";
import {
  ErrorBoundary,
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createRoot,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import { GrassBlock } from "../components/GrassBlock";
import { Icon, type IconName } from "../components/icons";
import { RmButton, RmIconButton, RmInput, RmProgress, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmEmptyState } from "../components/rm/surfaces";
import { ThemeHeroLayer } from "../components/ThemeSurface";
import { dashboard, downloads, type WidgetDescriptor } from "../ipc/api";
import type { VersionInfo } from "../ipc/types";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { formatBytes, formatDate, groupDigits, providerLabel, relativeTime } from "../lib/format";
import { loaderLabel } from "../lib/loaders";
import { playtimeLabel } from "../lib/playtime";
import { launchInstance } from "../lib/launchActions";
import { backgroundOnlyMode, toggleBackgroundOnlyMode } from "../lib/backgroundMode";
import { PluginFrame } from "../plugin-ui/PluginFrame";
import { authStore } from "../stores/auth";
import { downloadsStore, type DownloadBatch } from "../stores/downloads";
import { instancesStore } from "../stores/instances";
import { launchStore } from "../stores/launch";
import { confirmDanger, toast, uiStore } from "../stores/ui";
import * as versionCache from "../stores/versionCache";

const [catalog, setCatalog] = createRoot(() => createSignal<WidgetDescriptor[]>([]));

type BuiltinCardId =
  | "server"
  | "compare"
  | "announcements"
  | "instances"
  | "versions"
  | "downloads"
  | "games";

interface SoftLayout {
  order: string[];
  hidden: string[];
}

const SOFT_CARD_IDS = ["instances", "versions", "server", "downloads", "announcements", "compare", "games"];
const SOFT_DEFAULTS: SoftLayout = {
  order: ["versions", "server", "downloads"],
  hidden: ["instances", "announcements", "compare", "games"],
};
const SOFT_LAYOUT_KEY = "rmccc.softDashboard.v2";

function loadLayout(): SoftLayout {
  try {
    const stored = JSON.parse(localStorage.getItem(SOFT_LAYOUT_KEY) ?? "{}") as Partial<SoftLayout>;
    const order = Array.isArray(stored.order) ? stored.order : [...SOFT_DEFAULTS.order];
    for (const id of SOFT_DEFAULTS.order) if (!order.includes(id) && !(stored.hidden ?? []).includes(id)) order.push(id);
    return { order, hidden: Array.isArray(stored.hidden) ? stored.hidden : [] };
  } catch {
    return { order: [...SOFT_DEFAULTS.order], hidden: [...SOFT_DEFAULTS.hidden] };
  }
}

function saveLayout(layout: SoftLayout): void {
  try {
    localStorage.setItem(SOFT_LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    // The dashboard remains usable when storage is unavailable.
  }
}

export default function Home(): JSX.Element {
  const navigate = useNavigate();
  const [layout, setLayout] = createSignal<SoftLayout>(loadLayout());
  const [editing, setEditing] = createSignal(false);
  const [addOpen, setAddOpen] = createSignal(false);

  // Background-only mode hides the shell docks too (titlebar/account/launch)
  // so the wallpaper is unobstructed. The top-right switch stays visible.
  createEffect(() => {
    if (backgroundOnlyMode()) document.documentElement.dataset.backgroundOnly = "on";
    else delete document.documentElement.dataset.backgroundOnly;
  });
  onCleanup(() => {
    delete document.documentElement.dataset.backgroundOnly;
  });

  onMount(() => {
    void instancesStore.rescan();
    void authStore.refresh();
    void dashboard.listWidgets().then((res) => {
      if (res.ok) setCatalog(res.data);
      else toast({ message: res.error.message, tone: "error" });
    });
  });

  const selected = () => instancesStore.selected() ?? null;
  const session = () => authStore.session();
  const sortedByLastPlayed = createMemo(() =>
    [...(instancesStore.items() ?? [])].sort((a, b) =>
      (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")
    )
  );

  createEffect(() => {
    const items = instancesStore.items() ?? [];
    if (items.length === 0) return;
    const key = instancesStore.selectedKey();
    if (key && items.some((item) => item.summary.key === key)) return;
    const latest = [...items].sort((a, b) =>
      (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")
    )[0];
    if (latest) void instancesStore.selectInstance(latest.summary.key);
  });

  function persist(next: SoftLayout): void {
    setLayout(next);
    saveLayout(next);
  }

  function hideCard(id: string): void {
    const current = layout();
    persist({ order: current.order.filter((card) => card !== id), hidden: [...current.hidden, id] });
  }

  function moveCard(id: string, delta: -1 | 1): void {
    const order = [...layout().order];
    const index = order.indexOf(id);
    const target = index + delta;
    if (index < 0 || target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    persist({ ...layout(), order });
  }

  function restoreCard(id: string): void {
    const current = layout();
    if (!current.hidden.includes(id)) return;
    persist({ order: [...current.order, id], hidden: current.hidden.filter((card) => card !== id) });
  }

  async function resetLayout(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("home.resetLayout.title"),
      body: t("home.resetLayout.body"),
      confirmLabel: t("home.resetLayout.confirm"),
    });
    if (allowed) persist({ order: [...SOFT_DEFAULTS.order], hidden: [...SOFT_DEFAULTS.hidden] });
  }

  const pluginPrefix = "plugin:";
  function addCard(id: string): void {
    const current = layout();
    if (!current.order.includes(id)) persist({ order: [...current.order, id], hidden: current.hidden.filter((card) => card !== id) });
    setAddOpen(false);
  }

  function cardTitle(id: string): string {
    if (id.startsWith(pluginPrefix)) {
      const pluginId = id.slice(pluginPrefix.length);
      return catalog().find((widget) => widget.pluginId === pluginId)?.displayName ?? pluginId;
    }
    const titles: Record<BuiltinCardId, TranslationKey> = {
      server: "hero.server.title",
      compare: "hero.compare.title",
      announcements: "hero.ann.title",
      instances: "instances.title",
      versions: "home.allVersions",
      downloads: "tasks.sectionDownloads",
      games: "tasks.sectionGames",
    };
    return t(titles[id as BuiltinCardId]);
  }

  function cardIcon(id: string): IconName {
    if (id.startsWith(pluginPrefix)) return "extension";
    const icons: Record<BuiltinCardId, IconName> = {
      server: "activity", compare: "refresh", announcements: "info", instances: "gamepad",
      versions: "package", downloads: "download", games: "terminal",
    };
    return icons[id as BuiltinCardId];
  }

  const addableCards = createMemo(() => {
    const current = layout();
    const builtins = SOFT_CARD_IDS
      .filter((id) => !current.order.includes(id) || current.hidden.includes(id))
      .map((id) => ({ id, name: cardTitle(id), subtitle: "SNGL" }));
    const plugins = catalog()
      .filter((widget) => widget.pluginId !== "org.rmccc.core-ui")
      .map((widget) => ({ id: `${pluginPrefix}${widget.pluginId}`, name: widget.displayName, subtitle: widget.pluginId }))
      .filter((card) => !current.order.includes(card.id));
    return [...builtins, ...plugins];
  });

  const isRunning = () => launchStore.isRunning(selected()?.summary.key ?? "");
  const readiness = () => selected() ? (session() ? 100 : 72) : 0;

  return (
    <div class={`soft-home${backgroundOnlyMode() ? " is-background-only" : ""}`}>
      <ThemeHeroLayer />
      <header class="soft-header">
        <div class="soft-header-copy">
          <div class="soft-eyebrow"><span /> SNGL LAUNCHER</div>
          <h1>{t("soft.greeting", { name: session()?.username ?? "Player" })}<em>.</em></h1>
          <p>{t("soft.subtitle")}</p>
        </div>
        <div class="soft-header-actions">
          <button class="soft-search" onClick={() => navigate("/download#mods")}>
            <Icon name="search" size={17} /><span>{t("instances.search.placeholder")}</span>
          </button>
          <Show when={editing()}>
            <RmButton size="sm" icon="plus" onClick={() => setAddOpen(true)}>{t("home.addWidget")}</RmButton>
          </Show>
          <RmButton size="sm" icon={editing() ? "check" : "edit"} variant={editing() ? "primary" : "secondary"} onClick={() => setEditing(!editing())}>
            {editing() ? t("home.doneEditing") : t("home.editLayout")}
          </RmButton>
          <button
            class={`background-only-toggle${backgroundOnlyMode() ? " is-on" : ""}`}
            role="switch"
            aria-checked={backgroundOnlyMode()}
            aria-label={t("home.backgroundMode.title")}
            title={t("home.backgroundMode.title")}
            onClick={toggleBackgroundOnlyMode}
          >
            <Icon name={backgroundOnlyMode() ? "eye-off" : "eye"} size={16} />
            <span class="background-only-label">{t("home.backgroundMode")}</span>
            <span class="background-only-track" aria-hidden="true">
              <span class="background-only-knob" />
            </span>
          </button>
        </div>
      </header>

      <section class="soft-hero-grid">
        <article class="soft-card soft-current-card">
          <div class="soft-card-heading"><span>{t("soft.currentAdventure")}</span><RmIconButton icon="more-horizontal" label={t("common.more")} onClick={() => selected() && navigate(`/instances/${selected()!.summary.key}`)} /></div>
          <Show when={selected()} fallback={<div class="soft-empty-hero"><GrassBlock /><div><h2>{t("instances.empty.title")}</h2><p>{t("instances.empty.hint")}</p></div></div>}>
            {(profile) => (
              <div class="soft-current-content">
                <div class="soft-current-copy">
                  <div class="soft-status-pill"><i />{isRunning() ? t("instances.running") : t("soft.ready")}</div>
                  <h2>{profile().summary.display_name}</h2>
                  <p>{profile().summary.game_version}<Show when={profile().summary.loader}> · {loaderLabel(profile().summary.loader!.kind)}</Show></p>
                  <div class="soft-playtime"><Icon name="clock" size={15} />{profile().summary.play_time_seconds > 0 ? playtimeLabel(Number(profile().summary.play_time_seconds)) : t("playtime.none")}</div>
                </div>
                <GrassBlock />
              </div>
            )}
          </Show>
          <div class="soft-current-footer"><span>{instancesStore.selectedDirectory()?.directory.name ?? t("instances.root.profiles")}</span><span>{(instancesStore.items() ?? []).length} {t("instances.title")}</span></div>
        </article>

        <article class="soft-launch-card">
          <div class="soft-launch-orb o1" /><div class="soft-launch-orb o2" />
          <div class="soft-launch-top"><span>MINECRAFT</span><Icon name="sparkles" size={18} /></div>
          <div class="soft-launch-body">
            <div><p>{t("soft.playNow")}</p><h2>{selected()?.summary.game_version ?? "Minecraft"}</h2><span>{selected()?.summary.loader ? loaderLabel(selected()!.summary.loader!.kind) : t("instances.versionDetail.vanillaShort")}</span></div>
            <div class="soft-launch-cube"><GrassBlock /></div>
          </div>
          <div class="soft-launch-actions">
            <Show when={!isRunning()} fallback={<><RmButton variant="secondary" icon="terminal" onClick={() => uiStore.openLogDrawer(selected()?.summary.key ?? null)}>{t("home.viewLog")}</RmButton><RmButton variant="danger" icon="square" onClick={() => void launchStore.stop(selected()?.summary.key ?? "")}>{t("home.stop")}</RmButton></>}>
              <RmButton variant="primary" size="lg" icon={selected() ? "play" : "download"} loading={launchStore.isStarting(selected()?.summary.key ?? "")} onClick={() => void launchInstance(selected(), navigate)}>
                {selected() ? t("home.launch") : t("home.noInstance.cta")}
              </RmButton>
            </Show>
            <RmMenu align="right" side="up" items={[...sortedByLastPlayed().map((profile) => ({ label: `${profile.summary.display_name} · ${profile.summary.game_version}`, icon: "gamepad" as const, onSelect: () => void instancesStore.selectInstance(profile.summary.key) })), { label: t("home.instanceSettings"), icon: "sliders" as const, onSelect: () => selected() && navigate(`/instances/${selected()!.summary.key}#settings`) }]}>
              <button class="soft-launch-picker" aria-label={t("home.pickInstance")}><Icon name="layers" size={18} /><Icon name="chevron-down" size={13} /></button>
            </RmMenu>
          </div>
        </article>

        <article class="soft-card soft-status-card">
          <div class="soft-card-heading"><span>{t("soft.launchStatus")}</span><Icon name="activity" size={17} /></div>
          <div class="soft-status-ring" style={{ "--ready": `${readiness() * 3.6}deg` }}><div><strong>{readiness()}<small>%</small></strong><span>{t("soft.ready")}</span></div></div>
          <div class="soft-status-legend"><span><i class="is-green" />{t("hero.compare.upToDate")}</span><span><i class="is-blue" />{session() ? providerLabel(session()!.provider) : t("home.account.add")}</span></div>
        </article>
      </section>

      <section class="soft-dashboard-grid">
        <For each={layout().order}>
          {(id) => (
            <article class={`soft-card soft-module soft-module-${id.replace(":", "-")}`} data-card={id}>
              <div class="soft-card-heading">
                <span class="soft-module-title"><Icon name={cardIcon(id)} size={16} />{cardTitle(id)}</span>
                <Show when={!editing()}><span class="soft-card-dots"><i /><i /><i /></span></Show>
                <Show when={editing()}><div class="soft-edit-actions"><RmIconButton icon="chevron-up" label={t("common.moveUp")} onClick={() => moveCard(id, -1)} /><RmIconButton icon="chevron-down" label={t("common.moveDown")} onClick={() => moveCard(id, 1)} /><RmIconButton icon="x" label={t("common.hide")} onClick={() => hideCard(id)} /></div></Show>
              </div>
              <div class="soft-module-body">
                <Switch fallback={<ErrorBoundary fallback={(err, reset) => <div class="soft-plugin-error"><RmEmptyState icon="alert" title={t("home.card.broken")} hint={String(err)} /><RmButton size="sm" onClick={reset}>{t("common.retry")}</RmButton></div>}><PluginCard pluginId={id.slice(pluginPrefix.length)} /></ErrorBoundary>}>
                  <Match when={id === "server"}><ServerCard /></Match>
                  <Match when={id === "compare"}><CompareCard /></Match>
                  <Match when={id === "announcements"}><AnnouncementsCard /></Match>
                  <Match when={id === "instances"}><InstancesCard /></Match>
                  <Match when={id === "versions"}><VersionsCard /></Match>
                  <Match when={id === "downloads"}><DownloadsCard /></Match>
                  <Match when={id === "games"}><GamesCard /></Match>
                </Switch>
              </div>
            </article>
          )}
        </For>
      </section>

      <RmDialog open={addOpen()} title={t("home.addWidget")} onClose={() => setAddOpen(false)}>
        <div class="add-widget-list"><Show when={addableCards().length > 0} fallback={<div class="muted-line">{t("modStore.empty")}</div>}><For each={addableCards()}>{(card) => <button class="rm-row is-clickable" onClick={() => addCard(card.id)}><div class="rm-row-main"><div class="rm-row-title">{card.name}</div><div class="rm-row-subtitle">{card.subtitle}</div></div></button>}</For></Show></div>
      </RmDialog>
      <Show when={editing()}><div class="soft-editing-menu"><RmMenu align="right" items={[{ label: t("home.restoreHidden", { n: layout().hidden.length }), disabled: layout().hidden.length === 0, onSelect: () => restoreCard(layout().hidden[0]) }, { label: t("home.resetLayout.title"), danger: true, onSelect: () => void resetLayout() }]}><RmIconButton icon="more-horizontal" label={t("common.more")} /></RmMenu></div></Show>
    </div>
  );
}

function ServerCard(): JSX.Element {
  const [addr, setAddr] = createSignal("play.example.net");
  const [state, setState] = createSignal<"idle" | "loading" | "online" | "offline">("idle");
  const mock = { motd: "欢迎来到 Example 生存服务器！", players: { online: 42, max: 100 }, latencyMs: 23, serverVersion: "26.2 · Fabric 0.20.1" };
  function connect(): void {
    if (!addr().trim() || state() === "loading") return;
    setState("loading");
    setTimeout(() => setState(addr().includes("offline") ? "offline" : "online"), 800);
  }
  return <div class="soft-server"><div class="soft-server-input"><RmInput value={addr()} onInput={setAddr} placeholder={t("hero.server.address")} onEnter={connect} /><RmButton size="sm" variant="primary" icon="search" loading={state() === "loading"} onClick={connect}>{t("hero.server.connect")}</RmButton></div><Switch fallback={<div class="soft-empty">{t("hero.server.idle")}</div>}><Match when={state() === "loading"}><div class="soft-empty">{t("hero.server.probing")}</div></Match><Match when={state() === "offline"}><div class="soft-empty is-error">{t("hero.server.offline")}</div></Match><Match when={state() === "online"}><div class="soft-server-online"><strong>{mock.motd}</strong><div><span><Icon name="person" size={13} />{t("hero.server.players", { online: mock.players.online, max: mock.players.max })}</span><span><Icon name="activity" size={13} />{t("hero.server.latency", { ms: mock.latencyMs })}</span></div><small>{mock.serverVersion}</small></div></Match></Switch></div>;
}

function CompareCard(): JSX.Element {
  const clientVersion = () => instancesStore.selected()?.summary.game_version ?? "26.3";
  const serverVersion = "26.2";
  const needsUpdate = createMemo(() => clientVersion() < serverVersion);
  const [asked, setAsked] = createSignal(false);
  return <div class="soft-compare"><div><span>{t("hero.compare.client")}</span><strong>{clientVersion()}</strong></div><div><span>{t("hero.compare.server")}</span><strong>{serverVersion}</strong></div><div><span>{t("hero.compare.status")}</span><Show when={needsUpdate()} fallback={<RmTag tone="success">{t("hero.compare.upToDate")}</RmTag>}><RmTag tone="warning">{t("hero.compare.needsUpdate")}</RmTag></Show></div><Show when={needsUpdate()}><RmButton size="sm" variant="primary" icon="download" onClick={() => { setAsked(true); toast({ message: t("hero.compare.updateQueued"), tone: "info" }); }}>{t("hero.compare.update")}</RmButton></Show><Show when={asked()}><small>{t("hero.compare.queuedNote")}</small></Show></div>;
}

const MOCK_ANNOUNCEMENTS = [
  { date: "2026-10-24", tag: "update", text: "服务器已升级到 26.2：新增区块加载优化，建议同步更新客户端。" },
  { date: "2026-10-18", tag: "event", text: "周末建筑比赛报名开启，主题「四季花园」，奖金池 3000 金币。" },
  { date: "2026-10-10", tag: "notice", text: "10 月 12 日 02:00-04:00 例行停机维护，请提前下线。" },
];

function AnnouncementsCard(): JSX.Element {
  return <div class="soft-announcements"><For each={MOCK_ANNOUNCEMENTS}>{(item) => <div class="soft-announcement"><div><RmTag tone={item.tag === "update" ? "accent" : item.tag === "event" ? "success" : "neutral"}>{t(`hero.ann.tag.${item.tag}` as TranslationKey)}</RmTag><time>{item.date}</time></div><p>{item.text}</p></div>}</For></div>;
}

function InstancesCard(): JSX.Element {
  return <div class="soft-instance-list"><For each={[...(instancesStore.items() ?? [])].sort((a, b) => (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")).slice(0, 4)}>{(profile) => <button class={`soft-list-row${instancesStore.selected()?.summary.key === profile.summary.key ? " is-selected" : ""}`} onClick={() => void instancesStore.selectInstance(profile.summary.key)}><span class="soft-list-icon"><GrassBlock small /></span><span class="soft-list-copy"><strong>{profile.summary.display_name}</strong><small>{profile.summary.game_version}<Show when={profile.summary.loader}> · {loaderLabel(profile.summary.loader!.kind)}</Show></small></span><span class="soft-list-meta">{profile.summary.last_played ? relativeTime(profile.summary.last_played) : t("instances.neverPlayed")}</span><Icon name="chevron-right" size={14} /></button>}</For><Show when={(instancesStore.items() ?? []).length === 0}><div class="soft-empty">{t("instances.empty.title")}</div></Show></div>;
}

function VersionsCard(): JSX.Element {
  const navigate = useNavigate();
  const [versions, setVersions] = createSignal<VersionInfo[] | null>(null);
  const [pasted, setPasted] = createSignal("");
  onMount(() => {
    setVersions(versionCache.gameVersionsWithRefresh()?.filter((version) => version.release_type === "release") ?? null);
    void versionCache.refreshGameVersions().then((list) => { if (list) setVersions(list.filter((version) => version.release_type === "release")); });
  });
  const chips = createMemo(() => {
    const out: VersionInfo[] = [];
    const seen = new Set<string>();
    for (const version of versions() ?? []) {
      const major = version.id.split(".").slice(0, 2).join(".");
      if (seen.has(major)) continue;
      seen.add(major); out.push(version);
      if (out.length >= 4) break;
    }
    return out;
  });
  return <div class="soft-versions"><div class="soft-version-chips"><For each={chips()}>{(version) => <button onClick={() => navigate(`/download?version=${version.id}#install`)}><Icon name="package" size={15} /><strong>{version.id}</strong><small>{formatDate(version.release_time)}</small></button>}</For></div><div class="soft-version-paste"><RmInput value={pasted()} onInput={setPasted} placeholder={t("home.pasteVersion")} onEnter={() => pasted().trim() && navigate(`/download?version=${pasted().trim()}#install`)} /><RmIconButton icon="chevron-right" label={t("home.allVersions")} onClick={() => navigate("/download#install")} /></div></div>;
}

function DownloadsCard(): JSX.Element {
  const batches = createMemo(() => Object.values(downloadsStore.batches) as DownloadBatch[]);
  async function cancelBatch(batchId: string): Promise<void> {
    const allowed = await confirmDanger({ title: t("tasks.cancelConfirm.title"), body: t("tasks.cancelConfirm.body"), confirmLabel: t("tasks.download.cancel") });
    if (!allowed) return;
    const res = await downloads.cancel(batchId);
    toast({ message: res.ok ? t("tasks.download.cancelled") : res.error.message, tone: res.ok ? "info" : "error" });
  }
  return <div class="soft-downloads"><For each={batches()}>{(batch) => <div class="soft-download-row"><div><strong>{batch.label || batch.version}</strong><small>{groupDigits(batch.completedFiles)}/{groupDigits(batch.totalFiles)} · {formatBytes(batch.downloadedBytes)}/{formatBytes(batch.totalBytes)}</small></div><RmProgress value={batch.totalBytes > 0 ? (batch.downloadedBytes / batch.totalBytes) * 100 : 0} determinate={batch.totalBytes > 0} /><RmIconButton icon="x" label={t("tasks.download.cancel")} danger onClick={() => void cancelBatch(batch.batchId)} /></div>}</For><Show when={batches().length === 0}><div class="soft-empty"><Icon name="check" size={18} />{t("home.downloads.empty")}</div></Show></div>;
}

function GamesCard(): JSX.Element {
  return <div class="soft-instance-list"><For each={[...launchStore.running().values()]}>{(game) => <div class="soft-list-row"><span class="soft-list-icon"><span class="dot-running" /></span><span class="soft-list-copy"><strong>{instancesStore.items()?.find((profile) => profile.summary.key === game.profileId)?.summary.display_name ?? game.profileId}</strong><small>{t("instances.running")}</small></span><RmIconButton icon="terminal" label={t("home.viewLog")} onClick={() => uiStore.openLogDrawer(game.profileId)} /><RmIconButton icon="square" label={t("common.stop")} danger onClick={() => void launchStore.stop(game.profileId)} /></div>}</For><Show when={launchStore.running().size === 0}><div class="soft-empty">{t("tasks.empty")}</div></Show></div>;
}

function PluginCard(props: { pluginId: string }): JSX.Element {
  const descriptor = createMemo(() => catalog().find((widget) => widget.pluginId === props.pluginId));
  return <div class="soft-plugin-frame"><PluginFrame pluginId={props.pluginId} cardId="soft-dashboard" title={descriptor()?.displayName ?? props.pluginId} uiUrl={descriptor()?.uiUrl ?? `rmcccplugin://${props.pluginId}/ui/index.html`} /></div>;
}
