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
import { DownloadsCard } from "../components/home/DownloadsCard";
import { LaunchStatusCard } from "../components/home/LaunchStatusCard";
import { ServerCard } from "../components/home/ServerCard";
import { Icon, type IconName } from "../components/icons";
import { RmButton, RmIconButton, RmInput, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmEmptyState } from "../components/rm/surfaces";
import { ThemeHeroLayer } from "../components/ThemeSurface";
import { dashboard, type WidgetDescriptor } from "../ipc/api";
import type { VersionInfo } from "../ipc/types";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { formatDate, relativeTime } from "../lib/format";
import { loaderLabel } from "../lib/loaders";
import {
  type BuiltinCardId,
  type CardConstraints,
  type CardHeight,
  type CardPlacement,
  type CardWidth,
  type HomeLayoutV3,
  BUILTIN_CARD_IDS,
  DEFAULT_CARD_CONSTRAINTS,
  DEFAULT_CARD_SIZES,
  LAYOUT_KEY_V2,
  LAYOUT_KEY_V3,
  addCard as addCardToLayout,
  constraintsForBuiltin,
  cycleHeight,
  cycleWidth,
  defaultLayout,
  hideCard as hideCardInLayout,
  isBuiltinCardId,
  layoutCards,
  migrateLayoutV2,
  normalizePlacement,
  parseLayoutV3,
  parseLegacyPluginKey,
  parsePluginCardKey,
  pluginCardKey,
  reorderCard,
  restoreCard as restoreCardInLayout,
  setCardSize,
} from "../lib/cardLayout";
import { playtimeLabel } from "../lib/playtime";
import { launchInstance } from "../lib/launchActions";
import { backgroundOnlyMode, toggleBackgroundOnlyMode } from "../lib/backgroundMode";
import { PluginFrame } from "../plugin-ui/PluginFrame";
import { authStore } from "../stores/auth";
import { instancesStore } from "../stores/instances";
import { launchStore } from "../stores/launch";
import { confirmDanger, toast, uiStore } from "../stores/ui";
import * as versionCache from "../stores/versionCache";

const [catalog, setCatalog] = createRoot(() => createSignal<WidgetDescriptor[]>([]));

function localLayout(): HomeLayoutV3 {
  try {
    const storedV3 = localStorage.getItem(LAYOUT_KEY_V3);
    if (storedV3) {
      const parsed = parseLayoutV3(JSON.parse(storedV3));
      if (parsed) return parsed;
    }
    const storedV2 = localStorage.getItem(LAYOUT_KEY_V2);
    if (storedV2) {
      const migrated = migrateLayoutV2(JSON.parse(storedV2));
      if (migrated) {
        saveLocalLayout(migrated);
        return migrated;
      }
    }
  } catch {
    // Storage can be unavailable/corrupt; fall back to the canonical layout.
  }
  return defaultLayout();
}

function saveLocalLayout(layout: HomeLayoutV3): void {
  try {
    localStorage.setItem(LAYOUT_KEY_V3, JSON.stringify(layout));
  } catch {
    // The dashboard remains usable when storage is unavailable.
  }
}

function pluginCardIdOf(widget: WidgetDescriptor): string {
  const prefix = "dashboard-card:";
  return widget.widgetId.startsWith(prefix)
    ? widget.widgetId.slice(prefix.length)
    : widget.widgetId;
}

function descriptorForCard(catalog: WidgetDescriptor[], id: string): WidgetDescriptor | undefined {
  const ref = parsePluginCardKey(id);
  if (!ref) return undefined;
  return catalog.find(
    (widget) => widget.pluginId === ref.pluginId && pluginCardIdOf(widget) === ref.cardId
  );
}

function constraintsForDescriptor(widget: WidgetDescriptor): CardConstraints {
  const minW = Math.max(3, widget.minSize.w);
  const maxW = Math.min(12, Math.max(minW, widget.maxSize.w || 12));
  const minH = Math.max(1, widget.minSize.h);
  const maxH = Math.min(4, Math.max(minH, widget.maxSize.h || 4));
  return { minW, maxW, minH, maxH };
}

function placementForWidget(widget: WidgetDescriptor): CardPlacement {
  const id = pluginCardKey(widget.pluginId, pluginCardIdOf(widget));
  return normalizePlacement(
    id,
    widget.defaultSize.w || 4,
    widget.defaultSize.h || 2,
    constraintsForDescriptor(widget)
  );
}

/**
 * Resolve legacy `plugin:<pluginId>` entries to the plugin's first card and
 * clamp every placement to its declared/allowed size. Plugin descriptors only
 * arrive after `dashboard_list_widgets`, so this runs once the catalog lands.
 */
function reconcileLayout(current: HomeLayoutV3, widgets: WidgetDescriptor[]): HomeLayoutV3 {
  const catalogById = new Map<string, WidgetDescriptor>();
  for (const widget of widgets) {
    catalogById.set(pluginCardKey(widget.pluginId, pluginCardIdOf(widget)), widget);
  }

  const idMap = new Map<string, string>();
  const cards: CardPlacement[] = [];
  const seen = new Set<string>();
  for (const card of current.cards) {
    let id = card.id;
    const legacyPlugin = parseLegacyPluginKey(id);
    if (legacyPlugin) {
      const widget = widgets.find((item) => item.pluginId === legacyPlugin);
      if (widget) id = pluginCardKey(widget.pluginId, pluginCardIdOf(widget));
    }
    if (id !== card.id) idMap.set(card.id, id);
    if (seen.has(id)) continue;
    seen.add(id);
    const widget = catalogById.get(id);
    if (widget) {
      cards.push(normalizePlacement(id, card.w, card.h, constraintsForDescriptor(widget)));
    } else if (isBuiltinCardId(id)) {
      cards.push(normalizePlacement(id, card.w, card.h, constraintsForBuiltin(id)));
    } else {
      cards.push(card);
    }
  }

  const hidden = [...new Set(current.hidden.map((id) => idMap.get(id) ?? id))];
  return { schemaVersion: 3, cards, hidden };
}

export default function Home(): JSX.Element {
  const navigate = useNavigate();
  const [layout, setLayout] = createSignal<HomeLayoutV3>(localLayout());
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
      if (!res.ok) {
        toast({ message: res.error.message, tone: "error" });
        return;
      }
      setCatalog(res.data);
      const next = reconcileLayout(layout(), res.data);
      saveLocalLayout(next);
      setLayout(next);
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

  function persist(next: HomeLayoutV3): void {
    setLayout(next);
    saveLocalLayout(next);
  }

  function cardDescriptor(id: string): WidgetDescriptor | undefined {
    return descriptorForCard(catalog(), id);
  }

  function constraintsForCard(id: string): CardConstraints {
    const widget = cardDescriptor(id);
    if (widget) return constraintsForDescriptor(widget);
    if (isBuiltinCardId(id)) return constraintsForBuiltin(id);
    return DEFAULT_CARD_CONSTRAINTS;
  }

  function changeCardSize(id: string, axis: "w" | "h", delta: 1 | -1): void {
    const card = layout().cards.find((item) => item.id === id);
    if (!card) return;
    const constraints = constraintsForCard(id);
    const next: { w?: CardWidth; h?: CardHeight } = {};
    if (axis === "w") next.w = cycleWidth(card.w, delta, constraints);
    else next.h = cycleHeight(card.h, delta, constraints);
    persist(setCardSize(layout(), id, next));
  }

  function hideCard(id: string): void {
    persist(hideCardInLayout(layout(), id));
  }

  function restoreCard(id: string): void {
    persist(restoreCardInLayout(layout(), id));
  }

  function moveCard(id: string, delta: -1 | 1): void {
    persist(reorderCard(layout(), id, delta));
  }

  async function resetLayout(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("home.resetLayout.title"),
      body: t("home.resetLayout.body"),
      confirmLabel: t("home.resetLayout.confirm"),
    });
    if (allowed) persist(defaultLayout());
  }

  function placementForCardId(id: string): CardPlacement {
    const widget = cardDescriptor(id);
    if (widget) return placementForWidget(widget);
    if (isBuiltinCardId(id)) return { ...DEFAULT_CARD_SIZES[id] };
    return normalizePlacement(id, 4, 2);
  }

  function addCard(id: string): void {
    persist(addCardToLayout(layout(), placementForCardId(id)));
    setAddOpen(false);
  }

  function cardTitle(id: string): string {
    const widget = cardDescriptor(id);
    if (widget) return widget.displayName;
    const legacy = parseLegacyPluginKey(id);
    if (legacy) {
      return catalog().find((item) => item.pluginId === legacy)?.displayName ?? legacy;
    }
    if (!isBuiltinCardId(id)) return id;
    const titles: Record<BuiltinCardId, TranslationKey> = {
      server: "hero.server.title",
      compare: "hero.compare.title",
      announcements: "hero.ann.title",
      instances: "instances.title",
      versions: "home.allVersions",
      downloads: "tasks.sectionDownloads",
      games: "tasks.sectionGames",
    };
    return t(titles[id]);
  }

  function cardSubtitle(id: string): string {
    const widget = cardDescriptor(id);
    if (widget) return widget.pluginId;
    if (id.startsWith("plugin:")) return id.slice("plugin:".length);
    return "SNGL";
  }

  function cardIcon(id: string): IconName {
    if (id.startsWith("plugin:")) return "extension";
    if (!isBuiltinCardId(id)) return "grid";
    const icons: Record<BuiltinCardId, IconName> = {
      server: "activity",
      compare: "refresh",
      announcements: "info",
      instances: "gamepad",
      versions: "package",
      downloads: "download",
      games: "terminal",
    };
    return icons[id];
  }

  const addableCards = createMemo(() => {
    const current = layout();
    const visible = new Set(
      current.cards.filter((card) => !current.hidden.includes(card.id)).map((card) => card.id)
    );
    const builtins = BUILTIN_CARD_IDS
      .filter((id) => !visible.has(id))
      .map((id) => ({ id, name: cardTitle(id), subtitle: "SNGL" }));
    const plugins = catalog()
      .filter((widget) => widget.pluginId !== "org.rmccc.core-ui")
      .map((widget) => {
        const id = pluginCardKey(widget.pluginId, pluginCardIdOf(widget));
        return { id, name: widget.displayName, subtitle: widget.pluginId };
      })
      .filter((card) => !visible.has(card.id));
    return [...builtins, ...plugins];
  });

  const hiddenCards = createMemo(() =>
    layout().hidden.map((id) => ({ id, title: cardTitle(id), icon: cardIcon(id) }))
  );

  const isRunning = () => launchStore.isRunning(selected()?.summary.key ?? "");

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
                <div class="soft-current-art"><GrassBlock /></div>
              </div>
            )}
          </Show>
          <div class="soft-current-footer"><span>{instancesStore.selectedDirectory()?.directory.name ?? t("instances.root.profiles")}</span><span>{(instancesStore.items() ?? []).length} {t("instances.title")}</span></div>
        </article>

        <div class="soft-hero-side">
          <article class="soft-launch-card">
            <div class="soft-launch-orb o1" /><div class="soft-launch-orb o2" />
            <div class="soft-launch-top"><span>MINECRAFT · {t("soft.playNow")}</span><Icon name="sparkles" size={18} /></div>
            <div class="soft-launch-body">
              <div><p>{t("soft.currentVersion")}</p><h2>{selected()?.summary.game_version ?? "Minecraft"}</h2><span>{selected()?.summary.loader ? loaderLabel(selected()!.summary.loader!.kind) : t("instances.versionDetail.vanillaShort")}</span></div>
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

          <LaunchStatusCard />
        </div>
      </section>

      <section class="soft-dashboard-grid">
        <For each={layoutCards(layout())}>
          {(card) => {
            const ref = () => parsePluginCardKey(card.id);
            const descriptor = () => cardDescriptor(card.id);
            return (
              <article
                class={`soft-card soft-module soft-module-${card.id.replace(/[^a-z0-9]+/gi, "-")}`}
                data-card={card.id}
                data-w={card.w}
                data-h={card.h}
                style={{ "--card-w": `${card.w}`, "--card-h": `${card.h}` }}
              >
                <div class="soft-card-heading">
                  <span class="soft-module-title">
                    <Icon name={cardIcon(card.id)} size={16} />
                    {cardTitle(card.id)}
                  </span>
                  <Show when={!editing()}>
                    <span class="soft-card-dots"><i /><i /><i /></span>
                  </Show>
                  <Show when={editing()}>
                    <div class="soft-edit-actions">
                      <button
                        type="button"
                        class="soft-size-chip"
                        title={t("home.card.resizeWidth")}
                        aria-label={t("home.card.resizeWidth")}
                        onClick={() => changeCardSize(card.id, "w", 1)}
                      >
                        W{card.w}
                      </button>
                      <button
                        type="button"
                        class="soft-size-chip"
                        title={t("home.card.resizeHeight")}
                        aria-label={t("home.card.resizeHeight")}
                        onClick={() => changeCardSize(card.id, "h", 1)}
                      >
                        H{card.h}
                      </button>
                      <RmIconButton
                        icon="chevron-up"
                        label={t("common.moveUp")}
                        onClick={() => moveCard(card.id, -1)}
                      />
                      <RmIconButton
                        icon="chevron-down"
                        label={t("common.moveDown")}
                        onClick={() => moveCard(card.id, 1)}
                      />
                      <RmIconButton
                        icon="x"
                        label={t("common.hide")}
                        onClick={() => hideCard(card.id)}
                      />
                    </div>
                  </Show>
                </div>
                <div class="soft-module-body">
                  <ErrorBoundary
                    fallback={(err, reset) => (
                      <div class="soft-plugin-error">
                        <RmEmptyState
                          icon="alert"
                          title={t("home.card.broken")}
                          hint={String(err)}
                        />
                        <RmButton size="sm" onClick={reset}>{t("common.retry")}</RmButton>
                      </div>
                    )}
                  >
                    <Show
                      when={ref()}
                      fallback={
                        <Switch>
                          <Match when={card.id === "server"}><ServerCard /></Match>
                          <Match when={card.id === "compare"}><CompareCard /></Match>
                          <Match when={card.id === "announcements"}><AnnouncementsCard /></Match>
                          <Match when={card.id === "instances"}><InstancesCard /></Match>
                          <Match when={card.id === "versions"}><VersionsCard /></Match>
                          <Match when={card.id === "downloads"}><DownloadsCard /></Match>
                          <Match when={card.id === "games"}><GamesCard /></Match>
                        </Switch>
                      }
                    >
                      {(plugin) => (
                        <Show
                          when={descriptor()?.uiUrl}
                          fallback={
                            <div class="soft-empty">
                              <Icon name="extension" size={17} />
                              {descriptor()
                                ? t("home.card.noUi")
                                : t("home.card.pluginUnavailable")}
                            </div>
                          }
                        >
                          <PluginCard
                            pluginId={plugin().pluginId}
                            cardId={plugin().cardId}
                            descriptor={descriptor()}
                            width={card.w}
                            height={card.h}
                          />
                        </Show>
                      )}
                    </Show>
                  </ErrorBoundary>
                </div>
              </article>
            );
          }}
        </For>
      </section>

      <RmDialog open={addOpen()} title={t("home.addWidget")} onClose={() => setAddOpen(false)}>
        <div class="add-widget-list"><Show when={addableCards().length > 0} fallback={<div class="muted-line">{t("modStore.empty")}</div>}><For each={addableCards()}>{(card) => <button class="rm-row is-clickable" onClick={() => addCard(card.id)}><div class="rm-row-main"><div class="rm-row-title">{card.name}</div><div class="rm-row-subtitle">{card.subtitle}</div></div></button>}</For></Show></div>
      </RmDialog>
      <Show when={editing()}>
        <div class="soft-editing-menu">
          <RmMenu
            align="right"
            items={[
              ...(hiddenCards().length > 0
                ? hiddenCards().map((card) => ({
                    label: card.title,
                    icon: card.icon,
                    onSelect: () => restoreCard(card.id),
                  }))
                : [{
                    label: t("home.restoreHidden", { n: "0" }),
                    disabled: true,
                    onSelect: () => undefined,
                  }]),
              { label: t("home.resetLayout.title"), danger: true, onSelect: () => void resetLayout() },
            ]}
          >
            <RmIconButton icon="more-horizontal" label={t("common.more")} />
          </RmMenu>
        </div>
      </Show>
    </div>
  );
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

function GamesCard(): JSX.Element {
  return <div class="soft-instance-list"><For each={[...launchStore.running().values()]}>{(game) => <div class="soft-list-row"><span class="soft-list-icon"><span class="dot-running" /></span><span class="soft-list-copy"><strong>{instancesStore.items()?.find((profile) => profile.summary.key === game.profileId)?.summary.display_name ?? game.profileId}</strong><small>{t("instances.running")}</small></span><RmIconButton icon="terminal" label={t("home.viewLog")} onClick={() => uiStore.openLogDrawer(game.profileId)} /><RmIconButton icon="square" label={t("common.stop")} danger onClick={() => void launchStore.stop(game.profileId)} /></div>}</For><Show when={launchStore.running().size === 0}><div class="soft-empty">{t("tasks.empty")}</div></Show></div>;
}

function PluginCard(props: {
  pluginId: string;
  cardId: string;
  descriptor?: WidgetDescriptor;
  width: number;
  height: number;
}): JSX.Element {
  return (
    <div class="soft-plugin-frame">
      <PluginFrame
        pluginId={props.pluginId}
        cardId={props.cardId}
        title={props.descriptor?.displayName ?? props.pluginId}
        uiUrl={props.descriptor?.uiUrl ?? ""}
        card={{
          title: props.descriptor?.displayName ?? props.pluginId,
          width: props.width,
          height: props.height,
        }}
      />
    </div>
  );
}
