/**
 * Hero home (ui/home-hero-dock 试验分支) — 只重构主页，其他页面不变。
 *
 * 版面：全幅主题化背景（可经 --hero-image 换壁纸）；左侧是**可编辑的卡片栈**
 * （每个模块一张独立卡片：实例 / 版本 / 下载 / 游戏 + 插件卡片，支持隐藏、
 * 排序、添加、重置，持久化在 localStorage）；右下是账号卡与启动卡两张独立
 * 卡片。所有颜色走主题令牌（--rmccc-*），深浅主题自动适配。
 *
 * 启动链路与主分支一致：launchStore.start → 检查/补全资源 → 启动气泡。
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createRoot, onMount, Switch, Match } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { RmButton, RmIconButton, RmInput, RmProgress, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { Icon, type IconName } from "../components/icons";
import { AccountAvatar } from "../components/AccountAvatar";
import { PluginFrame } from "../plugin-ui/PluginFrame";
import { versions, dashboard, type WidgetDescriptor } from "../ipc/api";
import type { VersionInfo } from "../ipc/types";
import { profilesStore } from "../stores/profiles";
import { authStore } from "../stores/auth";
import { launchStore } from "../stores/launch";
import { downloadsStore } from "../stores/downloads";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { relativeTime, formatBytes, groupDigits, formatDate } from "../lib/format";

/** 插件/核心卡片目录（模块级共享：Home 装载，PluginCard 读取）。 */
const [catalog, setCatalog] = createRoot(() => createSignal<WidgetDescriptor[]>([]));

// ── 可编辑卡片布局（localStorage 持久化）──

type BuiltinCardId =
  | "server"        // 服务器信息（MOTD / 延迟 / 在线玩家）
  | "compare"       // 客户端 vs 服务端版本比对
  | "announcements" // 服务器更新公告
  | "instances"
  | "versions"
  | "downloads"
  | "games";

interface HeroLayout {
  order: string[];
  hidden: string[];
}

const HERO_DEFAULTS: HeroLayout = {
  order: ["server", "compare", "announcements"],
  hidden: ["instances", "versions", "downloads", "games"],
};
const HERO_LAYOUT_KEY = "rmccc.heroLayout.v2";

function loadLayout(): HeroLayout {
  try {
    const stored = JSON.parse(localStorage.getItem(HERO_LAYOUT_KEY) ?? "{}") as Partial<HeroLayout>;
    const order = Array.isArray(stored.order) ? stored.order : [...HERO_DEFAULTS.order];
    const hidden = Array.isArray(stored.hidden) ? stored.hidden : [];
    // 兜底：默认卡片丢了就补回来（新增内置卡片时自动出现）。
    for (const id of HERO_DEFAULTS.order) if (!order.includes(id)) order.push(id);
    return { order, hidden };
  } catch {
    return { ...HERO_DEFAULTS, order: [...HERO_DEFAULTS.order], hidden: [] };
  }
}

function saveLayout(layout: HeroLayout): void {
  try {
    localStorage.setItem(HERO_LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    /* localStorage 不可用时仅内存态 */
  }
}

export default function Home(): JSX.Element {
  const navigate = useNavigate();
  const [layout, setLayout] = createSignal<HeroLayout>(loadLayout());
  const [editing, setEditing] = createSignal(false);
  const [addOpen, setAddOpen] = createSignal(false);

  onMount(() => {
    void profilesStore.refresh();
    void authStore.refresh();
    void dashboard.listWidgets().then((res) => res.ok && setCatalog(res.data));
  });
  // 默认选中最近游玩的实例。
  createMemo(() => {
    if (!profilesStore.selectedId() && (profilesStore.items() ?? []).length > 0) {
      const latest = [...(profilesStore.items() ?? [])].sort((a, b) =>
        (b.last_played ?? "").localeCompare(a.last_played ?? "")
      )[0];
      if (latest) profilesStore.setSelectedId(latest.id);
    }
  });

  const selected = () => profilesStore.selected() ?? null;
  const session = () => authStore.session();

  function persist(next: HeroLayout): void {
    setLayout(next);
    saveLayout(next);
  }

  function hideCard(id: string): void {
    const current = layout();
    persist({
      order: current.order.filter((c) => c !== id),
      hidden: [...current.hidden, id],
    });
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
    persist({ order: [...current.order, id], hidden: current.hidden.filter((c) => c !== id) });
  }

  async function resetLayout(): Promise<void> {
    const allowed = await confirmDanger({
      title: t("home.resetLayout.title"),
      body: t("home.resetLayout.body"),
      confirmLabel: t("home.resetLayout.confirm"),
    });
    if (!allowed) return;
    persist({ ...HERO_DEFAULTS, order: [...HERO_DEFAULTS.order], hidden: [] });
  }

  /** 插件卡片：id 前缀区分，避免与内置卡片冲突。 */
  const pluginPrefix = "plugin:";

  function addCard(id: string): void {
    const current = layout();
    if (current.order.includes(id)) {
      setAddOpen(false);
      return;
    }
    persist({ order: [...current.order, id], hidden: current.hidden.filter((c) => c !== id) });
    setAddOpen(false);
  }

  function cardTitle(id: string): string {
    if (id.startsWith(pluginPrefix)) {
      const pluginId = id.slice(pluginPrefix.length);
      return catalog().find((w) => w.pluginId === pluginId)?.displayName ?? pluginId;
    }
    switch (id as BuiltinCardId) {
      case "server": return t("hero.server.title");
      case "compare": return t("hero.compare.title");
      case "announcements": return t("hero.ann.title");
      case "instances": return t("instances.title");
      case "versions": return t("home.allVersions");
      case "downloads": return t("tasks.sectionDownloads");
      case "games": return t("tasks.sectionGames");
    }
  }

  const addableCards = createMemo(() => {
    const current = layout();
    const builtins = HERO_DEFAULTS.order
      .filter((id) => !current.order.includes(id) || current.hidden.includes(id))
      .map((id) => ({ id, name: cardTitle(id), subtitle: "RMCCC" }));
    const plugins = catalog()
      .filter((w) => w.pluginId !== "org.rmccc.core-ui")
      .map((w) => ({ id: `${pluginPrefix}${w.pluginId}`, name: w.displayName, subtitle: w.pluginId }))
      .filter((c) => !current.order.includes(c.id));
    return [...builtins, ...plugins];
  });

  async function launchCurrent(): Promise<void> {
    const profile = selected();
    if (!profile) {
      navigate("/download#install");
      return;
    }
    if (!session()) {
      // WKWebView 不支持 window.prompt — 统一走 prompt 对话框。
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

  async function switchAccount(accountId: string): Promise<void> {
    const res = await authStore.switchAccount(accountId);
    toast({
      message: res.ok ? t("accounts.switched") : res.error.message,
      tone: res.ok ? "success" : "error",
    });
  }

  const accountMenuItems = () => [
    ...authStore
      .accounts()
      .filter((a) => a.account_id !== session()?.account_id)
      .map((a) => ({
        label: a.profile_name,
        icon: "person" as const,
        onSelect: () => void switchAccount(a.account_id),
      })),
    { label: t("accounts.manage"), icon: "settings" as const, onSelect: () => navigate("/accounts") },
    ...(session()
      ? [{ label: t("accounts.logout"), icon: "log-out" as const, danger: true, onSelect: () => void authStore.logout() }]
      : []),
  ];

  return (
    <div class="hero">
      {/* 流动背景光斑（纯 CSS transform 动画，主题色；reduced-motion 下静止） */}
      <div class="hero-flow" aria-hidden="true">
        <i class="hero-flow-blob is-a" />
        <i class="hero-flow-blob is-b" />
        <i class="hero-flow-blob is-c" />
      </div>

      {/* 编辑模式工具条 */}
      <div class="hero-toolbar">
        <Show when={editing()}>
          <RmButton size="sm" icon="plus" onClick={() => setAddOpen(true)}>
            {t("home.addWidget")}
          </RmButton>
          <RmMenu
            align="right"
            items={[
              {
                label: t("home.restoreHidden", { n: layout().hidden.length }),
                disabled: layout().hidden.length === 0,
                onSelect: () => restoreCard(layout().hidden[0]),
              },
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
          onClick={() => setEditing(!editing())}
        >
          {editing() ? t("home.doneEditing") : t("home.editLayout")}
        </RmButton>
      </div>

      {/* 左侧：可编辑卡片栈 */}
      <div class="hero-stack">
        <For each={layout().order}>
          {(id) => (
            <section class="hero-card hero-module" data-card={id}>
              <Show when={editing()}>
                <div class="hero-module-bar">
                  <span class="hero-module-name">{cardTitle(id)}</span>
                  <div class="btn-row">
                    <RmIconButton icon="chevron-up" label={t("common.moveUp")} onClick={() => moveCard(id, -1)} />
                    <RmIconButton icon="chevron-down" label={t("common.moveDown")} onClick={() => moveCard(id, 1)} />
                    <RmIconButton icon="x" label={t("common.hide")} onClick={() => hideCard(id)} />
                  </div>
                </div>
              </Show>
              <div class="hero-module-body">
                <Switch fallback={<PluginCard pluginId={id.slice(pluginPrefix.length)} />}>
                  <Match when={id === "server"}><ServerCard /></Match>
                  <Match when={id === "compare"}><CompareCard /></Match>
                  <Match when={id === "announcements"}><AnnouncementsCard /></Match>
                  <Match when={id === "instances"}><InstancesCard /></Match>
                  <Match when={id === "versions"}><VersionsCard /></Match>
                  <Match when={id === "downloads"}><DownloadsCard /></Match>
                  <Match when={id === "games"}><GamesCard /></Match>
                </Switch>
              </div>
            </section>
          )}
        </For>
      </div>

      {/* 右下：账号卡 + 启动卡（各自独立的卡片） */}
      <div class="hero-corner">
        <Show
          when={session()}
          fallback={
            <button class="hero-card hero-module" onClick={() => navigate("/accounts")}>
              <span class="hero-card-icon"><Icon name="person" size={18} /></span>
              <span class="hero-card-main">
                <span class="hero-card-title">{t("home.account.signedOut")}</span>
                <span class="hero-card-subtitle">{t("home.account.add")}</span>
              </span>
              <span class="hero-card-plus">+</span>
            </button>
          }
        >
          {(current) => (
            <RmMenu align="right" side="up" items={accountMenuItems()}>
              <button class="hero-card hero-module hero-account">
                <AccountAvatar uuid={current().uuid} name={current().username} provider={current().provider} />
                <span class="hero-card-main">
                  <span class="hero-card-title">{current().username}</span>
                  <span class="hero-account-meta">
                    <RmTag tone="accent">{current().provider}</RmTag>
                    <span class="hero-account-meta-item">
                      {t("home.account.lastPlayed", {
                        time: authStore.lastPlayedFor(current().account_id)
                          ? relativeTime(authStore.lastPlayedFor(current().account_id))
                          : t("instances.neverPlayed"),
                      })}
                    </span>
                  </span>
                  <span class="hero-account-meta">
                    <span class="hero-account-meta-item">
                      {t("hero.account.instances", { n: (profilesStore.items() ?? []).length })}
                    </span>
                    <span class="hero-account-meta-item">
                      {t("hero.account.id", { id: current().account_id.replace(/^account:/, "").slice(0, 8) })}
                    </span>
                  </span>
                </span>
                <span class="hero-card-plus"><Icon name="chevron-down" size={14} /></span>
              </button>
            </RmMenu>
          )}
        </Show>

        <div class="hero-card hero-module hero-launch">
          <div class="hero-launch-main">
            <RmButton
              variant="primary"
              icon="play"
              size="lg"
              loading={launchStore.isStarting(selected()?.id ?? "")}
              onClick={() => void launchCurrent()}
            >
              {t("home.launch")}
            </RmButton>
            <RmMenu
              align="right"
              side="up"
              items={[
                ...(profilesStore.items() ?? [])
                  .sort((a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? ""))
                  .map((p) => ({
                    label: `${p.name} · ${p.game_version}`,
                    icon: "gamepad" as const,
                    onSelect: () => profilesStore.setSelectedId(p.id),
                  })),
                { label: t("download.wizard.bindLocal"), icon: "download" as const, onSelect: () => navigate("/download#install") },
                ...(selected()
                  ? [{ label: t("home.instanceSettings"), icon: "sliders" as const, onSelect: () => navigate(`/instances/${selected()!.id}#settings`) }]
                  : []),
              ]}
            >
              <button class="hero-launch-picker">
                <span class="hero-launch-picker-main">
                  {selected()?.name ?? t("home.pickInstance")}
                  <Show when={selected()}>
                    {(p) => (
                      <span class="hero-launch-picker-sub">
                        {p().game_version}
                        <Show when={p().mod_loader}> · {p().mod_loader!.loader_type}</Show>
                      </span>
                    )}
                  </Show>
                </span>
                <Icon name="chevron-down" size={14} />
              </button>
            </RmMenu>
          </div>
          <Show when={launchStore.isRunning(selected()?.id ?? "")}>
            <div class="hero-launch-running">
              <span class="dot-running" /> {t("instances.running")}
              <RmButton size="sm" variant="ghost" icon="terminal" onClick={() => uiStore.openLogDrawer(selected()?.id ?? null)}>
                {t("home.viewLog")}
              </RmButton>
            </div>
          </Show>
        </div>
      </div>

      {/* 添加卡片对话框 */}
      <RmDialog open={addOpen()} title={t("home.addWidget")} onClose={() => setAddOpen(false)}>
        <div class="add-widget-list">
          <Show when={addableCards().length > 0} fallback={<div class="muted-line">{t("modStore.empty")}</div>}>
            <For each={addableCards()}>
              {(card) => (
                <button class="rm-row is-clickable" onClick={() => addCard(card.id)}>
                  <div class="rm-row-main">
                    <div class="rm-row-title">{card.name}</div>
                    <div class="rm-row-subtitle">{card.subtitle}</div>
                  </div>
                </button>
              )}
            </For>
          </Show>
        </div>
      </RmDialog>
    </div>
  );
}

// ── 内置卡片内容 ──

// ── 服务器信息卡（演示数据，后端未接入）──

function ServerCard(): JSX.Element {
  const [addr, setAddr] = createSignal("play.example.net");
  const [state, setState] = createSignal<"idle" | "loading" | "online" | "offline">("idle");
  // 模拟查询结果（接后端后由 ping 响应填充）。
  const mock = {
    motd: "欢迎来到 Example 生存服务器！",
    players: { online: 42, max: 100 },
    latencyMs: 23,
    serverVersion: "26.2 · Fabric 0.20.1",
  };

  function connect(): void {
    if (!addr().trim() || state() === "loading") return;
    setState("loading");
    setTimeout(() => {
      // 演示：地址含 "offline" 时模拟连不上。
      setState(addr().includes("offline") ? "offline" : "online");
    }, 800);
  }

  return (
    <div class="hero-server">
      <div class="btn-row">
        <RmInput
          value={addr()}
          onInput={setAddr}
          placeholder={t("hero.server.address")}
          onEnter={() => void connect()}
        />
        <RmButton size="sm" variant="primary" icon="search" loading={state() === "loading"} onClick={() => void connect()}>
          {t("hero.server.connect")}
        </RmButton>
      </div>
      <Switch
        fallback={<div class="hero-empty">{t("hero.server.idle")}</div>}
      >
        <Match when={state() === "loading"}>
          <div class="hero-empty">{t("hero.server.probing")}</div>
        </Match>
        <Match when={state() === "offline"}>
          <div class="hero-empty is-error">{t("hero.server.offline")}</div>
        </Match>
        <Match when={state() === "online"}>
          <div class="hero-server-info">
            <div class="hero-server-motd">{mock.motd}</div>
            <div class="hero-server-stats">
              <span class="hero-server-stat"><Icon name="person" size={14} /> {t("hero.server.players", { online: mock.players.online, max: mock.players.max })}</span>
              <span class="hero-server-stat"><Icon name="check" size={14} /> {t("hero.server.latency", { ms: mock.latencyMs })}</span>
              <span class="hero-server-stat"><Icon name="package" size={14} /> {mock.serverVersion}</span>
            </div>
          </div>
        </Match>
      </Switch>
    </div>
  );
}

// ── 客户端 / 服务端版本比对卡（演示数据）──

function CompareCard(): JSX.Element {
  const clientVersion = () => profilesStore.selected()?.game_version ?? "26.3";
  // 模拟：服务端版本固定 26.2，客户端更高则无需更新。
  const serverVersion = "26.2";
  const needsUpdate = createMemo(() => clientVersion() < serverVersion);
  const [asked, setAsked] = createSignal(false);

  return (
    <div class="hero-compare">
      <div class="hero-compare-row">
        <span class="hero-compare-label">{t("hero.compare.client")}</span>
        <span class="hero-compare-value">{clientVersion()}</span>
      </div>
      <div class="hero-compare-row">
        <span class="hero-compare-label">{t("hero.compare.server")}</span>
        <span class="hero-compare-value">{serverVersion}</span>
      </div>
      <div class="hero-compare-row">
        <span class="hero-compare-label">{t("hero.compare.status")}</span>
        <Show
          when={needsUpdate()}
          fallback={<RmTag tone="success">{t("hero.compare.upToDate")}</RmTag>}
        >
          <RmTag tone="warning">{t("hero.compare.needsUpdate")}</RmTag>
        </Show>
      </div>
      <Show when={needsUpdate()}>
        <div class="btn-row hero-compare-actions">
          <RmButton
            size="sm"
            variant="primary"
            icon="download"
            onClick={() => {
              setAsked(true);
              toast({ message: t("hero.compare.updateQueued"), tone: "info" });
            }}
          >
            {t("hero.compare.update")}
          </RmButton>
          <RmButton size="sm" variant="ghost" onClick={() => setAsked(false)}>
            {t("common.cancel")}
          </RmButton>
        </div>
      </Show>
      <Show when={asked()}>
        <div class="muted-line">{t("hero.compare.queuedNote")}</div>
      </Show>
    </div>
  );
}

// ── 服务器更新公告卡（演示数据）──

const MOCK_ANNOUNCEMENTS = [
  { date: "2026-10-24", tag: "update", text: "服务器已升级到 26.2：新增区块加载优化，建议同步更新客户端。" },
  { date: "2026-10-18", tag: "event", text: "周末建筑比赛报名开启，主题「四季花园」，奖金池 3000 金币。" },
  { date: "2026-10-10", tag: "notice", text: "10 月 12 日 02:00-04:00 例行停机维护，请提前下线。" },
];

function AnnouncementsCard(): JSX.Element {
  return (
    <div class="hero-ann-list">
      <For each={MOCK_ANNOUNCEMENTS}>
        {(item) => (
          <div class="hero-ann-item">
            <div class="hero-ann-head">
              <RmTag tone={item.tag === "update" ? "accent" : item.tag === "event" ? "success" : "neutral"}>
                {t(`hero.ann.tag.${item.tag}` as TranslationKey)}
              </RmTag>
              <span class="hero-ann-date">{item.date}</span>
            </div>
            <div class="hero-ann-text">{item.text}</div>
          </div>
        )}
      </For>
    </div>
  );
}

function InstancesCard(): JSX.Element {
  return (
    <div class="hero-list">
      <For each={profilesStore.items() ?? []}>
        {(profile) => (
          <button
            class={`hero-row${(profilesStore.selected()?.id ?? null) === profile.id ? " is-selected" : ""}`}
            onClick={() => profilesStore.setSelectedId(profile.id)}
          >
            <span class="hero-row-icon"><Icon name="gamepad" size={16} /></span>
            <span class="hero-row-main">
              <span class="hero-row-title">{profile.name}</span>
              <span class="hero-row-subtitle">
                {profile.game_version}
                <Show when={profile.mod_loader}> · {profile.mod_loader!.loader_type}</Show>
              </span>
            </span>
            <span class="hero-row-meta">
              {profile.last_played ? relativeTime(profile.last_played) : t("instances.neverPlayed")}
            </span>
          </button>
        )}
      </For>
      <Show when={(profilesStore.items() ?? []).length === 0}>
        <div class="hero-empty">{t("instances.empty.title")}</div>
      </Show>
    </div>
  );
}

function VersionsCard(): JSX.Element {
  const navigate = useNavigate();
  const [versions_, setVersions_] = createSignal<VersionInfo[] | null>(null);
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
      if (out.length >= 8) break;
    }
    return out;
  });
  return (
    <div class="hero-list">
      <For each={chips()}>
        {(version) => (
          <button class="hero-row" onClick={() => navigate(`/download#install?version=${version.id}`)}>
            <span class="hero-row-icon"><Icon name="package" size={16} /></span>
            <span class="hero-row-main">
              <span class="hero-row-title">{version.id}</span>
              <span class="hero-row-subtitle">{formatDate(version.release_time)}</span>
            </span>
            <RmTag tone="accent">{t("common.install")}</RmTag>
          </button>
        )}
      </For>
      <button class="hero-row" onClick={() => navigate("/download#install")}>
        <span class="hero-row-icon"><Icon name="download" size={16} /></span>
        <span class="hero-row-main">
          <span class="hero-row-title">{t("home.allVersions")}</span>
        </span>
      </button>
    </div>
  );
}

function DownloadsCard(): JSX.Element {
  const batches = createMemo(() => [...downloadsStore.batches().values()]);
  return (
    <div class="hero-list">
      <For each={batches()}>
        {(batch) => (
          <div class="hero-row is-static">
            <span class="hero-row-main">
              <span class="hero-row-title">{batch.label || batch.version}</span>
              <span class="hero-row-subtitle">
                {groupDigits(batch.completedFiles)}/{groupDigits(batch.totalFiles)} ·{" "}
                {formatBytes(batch.downloadedBytes)}/{formatBytes(batch.totalBytes)}
                <Show when={batch.speed > 0}> · {formatBytes(batch.speed)}/s</Show>
              </span>
            </span>
            <RmProgress
              value={batch.totalBytes > 0 ? (batch.downloadedBytes / batch.totalBytes) * 100 : 0}
              determinate={batch.totalBytes > 0}
            />
          </div>
        )}
      </For>
      <Show when={batches().length === 0}>
        <div class="hero-empty">{t("home.downloads.empty")}</div>
      </Show>
    </div>
  );
}

function GamesCard(): JSX.Element {
  const navigate = useNavigate();
  return (
    <div class="hero-list">
      <For each={[...launchStore.running().values()]}>
        {(game) => (
          <div class="hero-row is-static">
            <span class="hero-row-icon"><span class="dot-running" /></span>
            <span class="hero-row-main">
              <span class="hero-row-title">
                {profilesStore.items()?.find((p) => p.id === game.profileId)?.name ?? game.profileId}
              </span>
              <span class="hero-row-subtitle">{t("instances.running")}</span>
            </span>
            <RmIconButton icon="terminal" label={t("home.viewLog")} onClick={() => uiStore.openLogDrawer(game.profileId)} />
            <RmIconButton icon="square" label={t("common.stop")} danger onClick={() => void launchStore.kill(game.profileId)} />
          </div>
        )}
      </For>
      <Show when={launchStore.running().size === 0}>
        <div class="hero-empty">{t("tasks.empty")}</div>
      </Show>
      <Show when={launchStore.running().size > 0}>
        <button class="hero-row" onClick={() => navigate("/instances")}>
          <span class="hero-row-icon"><Icon name="gamepad" size={16} /></span>
          <span class="hero-row-main">
            <span class="hero-row-title">{t("instances.title")}</span>
          </span>
        </button>
      </Show>
    </div>
  );
}

function PluginCard(props: { pluginId: string }): JSX.Element {
  const descriptor = createMemo(() =>
    catalog().find((w) => w.pluginId === props.pluginId)
  );
  const uiUrl = () =>
    descriptor()?.uiUrl ?? `rmcccplugin://${props.pluginId}/ui/index.html`;
  return (
    <div class="hero-plugin-frame">
      <PluginFrame
        pluginId={props.pluginId}
        cardId="hero"
        title={descriptor()?.displayName ?? props.pluginId}
        uiUrl={uiUrl()}
      />
    </div>
  );
}
