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
import { Show, For, createMemo, createSignal, createRoot, createEffect, onMount, Switch, Match, ErrorBoundary } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { RmButton, RmIconButton, RmInput, RmProgress, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmEmptyState } from "../components/rm/surfaces";
import { Icon, type IconName } from "../components/icons";
import { AccountAvatar } from "../components/AccountAvatar";
import { PluginFrame } from "../plugin-ui/PluginFrame";
import { ThemeHeroLayer } from "../components/ThemeSurface";
import { dashboard, external, downloads, type WidgetDescriptor } from "../ipc/api";
import type { VersionInfo } from "../ipc/types";
import { instancesStore } from "../stores/instances";
import { authStore } from "../stores/auth";
import { launchStore } from "../stores/launch";
import { downloadsStore } from "../stores/downloads";
import type { DownloadBatch } from "../stores/downloads";
import * as versionCache from "../stores/versionCache";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { settingsStore } from "../stores/settings";
import { themeMode } from "../theme";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { relativeTime, formatBytes, groupDigits, formatDate, providerLabel } from "../lib/format";
import { playtimeLabel } from "../lib/playtime";
import { loaderLabel } from "../lib/loaders";

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

// ── 主题化环境动效（2026-10-25）────────────────────────────────
// 每套内置主题配一组与名字呼应的 CSS 意象粒子：林间落叶 / 樱吹雪飘瓣 /
// 深海气泡 / 熔岩火星 / 靛夜星辰 / 晴空流云 / 纸墨浮尘。全部纯 CSS
// transform/opacity 动画（无 JS 每帧循环），受 data-motion 闸门控制。

type ParticleSpec = {
  left: string;      // 水平起点
  delay: number;     // 动画延迟（负值 = 开局即处于中段）
  sway?: string;     // 水平摆动幅度
  spin?: string;     // 旋转角
};

/** 伪随机粒子参数（固定种子序列，布局稳定不跳变）。 */
function specs(count: number, salt: number): ParticleSpec[] {
  const r = (n: number) => {
    const x = Math.sin(n * 127.1 + salt * 311.7) * 43758.5453;
    return x - Math.floor(x);
  };
  return Array.from({ length: count }, (_, i) => ({
    left: `${Math.round(r(i + 1) * 96)}%`,
    delay: -r(i + 31) * 24,
    sway: `${Math.round(20 + r(i + 51) * 70)}px`,
    spin: `${Math.round(220 + r(i + 71) * 260)}deg`,
  }));
}

function particleStyle(p: ParticleSpec, size: number, duration: [number, number]): JSX.CSSProperties {
  return {
    left: p.left,
    width: `${size}px`,
    height: `${size}px`,
    "animation-delay": `${p.delay.toFixed(2)}s`,
    "animation-duration": `${(duration[0] + (p.delay % 1 + 1) % 1 * (duration[1] - duration[0])).toFixed(2)}s`,
    "--sway": p.sway,
    "--spin": p.spin,
  };
}

/** 主题 key → hero-flow 容器类（光斑按主题适配的 CSS 钩子）。 */
function ambientThemeClass(key: string): string {
  const known = [
    "builtin:dark",
    "builtin:forest",
    "builtin:ocean",
    "builtin:ember",
    "builtin:sakura",
    "builtin:paper",
    "builtin:sky",
    "builtin:light",
  ];
  if (known.includes(key)) return `t-${key.replace("builtin:", "")}`;
  return "t-dark"; // 插件主题等未知 key 沿用暗色底光
}

/** 当前生效的内置主题 key（null/跟随系统按已应用的明暗模式解析；mode 来自
 *  theme 模块的 signal，OS 明暗切换时氛围层随之更新——N-11）。 */
function useAmbientKey() {
  return createMemo(() => {
    const pref = settingsStore.themePreference();
    if (pref && pref !== "system") return pref;
    return themeMode() === "light" ? "builtin:light" : "builtin:dark";
  });
}

function ThemeAmbient(props: { themeKey: string }): JSX.Element {
  const key = () => props.themeKey;
  return (
    <Switch>
      <Match when={key() === "builtin:forest"}><><ForestAmbient /><ForestGlow /></></Match>
      <Match when={key() === "builtin:sakura"}><><SakuraAmbient /><SakuraGlow /></></Match>
      <Match when={key() === "builtin:ocean"}><><OceanAmbient /><OceanGlow /></></Match>
      <Match when={key() === "builtin:ember"}><><EmberAmbient /><EmberGlow /></></Match>
      <Match when={key() === "builtin:dark"}><><StarAmbient /><DefaultGlow /></></Match>
      <Match when={key() === "builtin:sky"}><><SkyAmbient /><SkyGlow /></></Match>
      <Match when={key() === "builtin:paper"}><><PaperAmbient /><PaperGlow /></></Match>
      {/* 晨白（light）：无粒子，只保留令牌驱动的三团光斑 */}
      <Match when={key() === "builtin:light"}><DefaultGlow /></Match>
      {/* 插件/未知主题按生效明暗回退：暗色用星野，亮色只保留令牌驱动的光斑。 */}
      <Match when={true}>
        <Show when={themeMode() === "light"} fallback={<><StarAmbient /><DefaultGlow /></>}>
          <DefaultGlow />
        </Show>
      </Match>
    </Switch>
  );
}

/** 令牌驱动的三团光斑（靛夜 / 晨白的专属设计：颜色随 --rmccc-* 自动适配）。 */
function DefaultGlow(): JSX.Element {
  return (
    <div class="hero-glow g-default">
      <i class="hero-flow-blob is-a" />
      <i class="hero-flow-blob is-b" />
      <i class="hero-flow-blob is-c" />
    </div>
  );
}

/** 林间：两条树隙光柱 + 底部苔绿雾光呼吸。 */
function ForestGlow(): JSX.Element {
  return (
    <div class="hero-glow g-forest">
      <i class="g-beam is-a" />
      <i class="g-beam is-b" />
      <i class="g-fog" />
    </div>
  );
}

/** 樱吹雪：右上粉霞 + 左下白雾，极慢对角呼吸。 */
function SakuraGlow(): JSX.Element {
  return (
    <div class="hero-glow g-sakura">
      <i class="g-haze" />
      <i class="g-mist" />
    </div>
  );
}

/** 深海：顶部水面光 + 上升光柱 + 底部深蓝压边。 */
function OceanGlow(): JSX.Element {
  return (
    <div class="hero-glow g-ocean">
      <i class="g-surface" />
      <i class="g-shaft is-a" />
      <i class="g-shaft is-b" />
      <i class="g-abyss" />
    </div>
  );
}

/** 熔岩：底部双团岩浆辉光交替呼吸 + 热浪层横移。 */
function EmberGlow(): JSX.Element {
  return (
    <div class="hero-glow g-ember">
      <i class="g-magma is-a" />
      <i class="g-magma is-b" />
      <i class="g-heat" />
    </div>
  );
}

/** 晴空：左上太阳辉光 + 两条淡云带缓慢横移。 */
function SkyGlow(): JSX.Element {
  return (
    <div class="hero-glow g-sky">
      <i class="g-sun" />
      <i class="g-band is-a" />
      <i class="g-band is-b" />
    </div>
  );
}

/** 纸墨：暖纸晕染 + 淡金漂移（近乎静态的纸面光影）。 */
function PaperGlow(): JSX.Element {
  return (
    <div class="hero-glow g-paper">
      <i class="g-wash" />
      <i class="g-gold" />
    </div>
  );
}

/** 林间：叶形（对角圆角）旋转飘落。 */
function ForestAmbient(): JSX.Element {
  const leaves = specs(10, 1);
  return (
    <div class="hero-particles p-forest">
      <For each={leaves}>
        {(p, i) => (
          <i class="p-leaf" style={{ ...particleStyle(p, 10 + (i() % 3) * 6, [9, 16]), top: "-6%" }} />
        )}
      </For>
    </div>
  );
}

/** 樱吹雪：圆缺花瓣（花瓣形 border-radius）斜向飘落。 */
function SakuraAmbient(): JSX.Element {
  const petals = specs(12, 2);
  return (
    <div class="hero-particles p-sakura">
      <For each={petals}>
        {(p, i) => (
          <i class="p-petal" style={{ ...particleStyle(p, 9 + (i() % 3) * 5, [8, 15]), top: "-6%" }} />
        )}
      </For>
    </div>
  );
}

/** 深海：描边气泡自下而上漂浮。 */
function OceanAmbient(): JSX.Element {
  const bubbles = specs(10, 3);
  return (
    <div class="hero-particles p-ocean">
      <For each={bubbles}>
        {(p, i) => (
          <i class="p-bubble" style={{ ...particleStyle(p, 6 + (i() % 4) * 6, [10, 18]), top: "104%" }} />
        )}
      </For>
    </div>
  );
}

/** 熔岩：发光火星上浮渐隐。 */
function EmberAmbient(): JSX.Element {
  const embers = specs(9, 4);
  return (
    <div class="hero-particles p-ember">
      <For each={embers}>
        {(p, i) => (
          <i class="p-ember-dot" style={{ ...particleStyle(p, 4 + (i() % 3) * 3, [6, 12]), top: "104%" }} />
        )}
      </For>
    </div>
  );
}

/** 靛夜：星辰闪烁（不下坠，只闪 + 微移）。 */
function StarAmbient(): JSX.Element {
  const stars = specs(14, 5);
  return (
    <div class="hero-particles p-dark">
      <For each={stars}>
        {(p) => (
          <i
            class="p-star"
            style={{
              left: p.left,
              top: `${Math.round(Math.abs(p.delay) * 38)}%`,
              width: "2px",
              height: "2px",
              "animation-delay": `${p.delay.toFixed(2)}s`,
              "animation-duration": `${(3 + (Math.abs(p.delay) % 3)).toFixed(2)}s`,
            }}
          />
        )}
      </For>
    </div>
  );
}

/** 晴空：大团流云缓慢横移。 */
function SkyAmbient(): JSX.Element {
  const clouds = specs(4, 6);
  return (
    <div class="hero-particles p-sky">
      <For each={clouds}>
        {(p, i) => (
          <i
            class="p-cloud"
            style={{
              left: "-20%",
              top: `${8 + i() * 22}%`,
              width: `${180 + i() * 90}px`,
              height: `${54 + i() * 16}px`,
              "animation-delay": `${(p.delay * -1.4).toFixed(2)}s`,
              "animation-duration": `${(70 + i() * 26).toFixed(0)}s`,
            }}
          />
        )}
      </For>
    </div>
  );
}

/** 纸墨：淡墨尘点极缓慢漂移（几乎不可察觉的层次感）。 */
function PaperAmbient(): JSX.Element {
  const dust = specs(10, 7);
  return (
    <div class="hero-particles p-paper">
      <For each={dust}>
        {(p, i) => (
          <i
            class="p-dust"
            style={{
              left: p.left,
              top: `${(Math.abs(p.delay) * 89) % 92}%`,
              width: `${2 + (i() % 3)}px`,
              height: `${2 + (i() % 3)}px`,
              "animation-delay": `${p.delay.toFixed(2)}s`,
              "animation-duration": `${(14 + Math.abs(p.delay) % 10).toFixed(1)}s`,
            }}
          />
        )}
      </For>
    </div>
  );
}

/** Decorative three-color voxel scene. Pure CSS geometry; never receives input. */
function VoxelScene(): JSX.Element {
  return (
    <div class="hero-voxel-scene" aria-hidden="true">
      <div class="hero-grid-plane" />
      <div class="hero-orbit is-outer"><i /><i /><i /></div>
      <div class="hero-orbit is-inner"><i /><i /></div>
      <div class="voxel-stage">
        <div class="voxel-cube">
          <i class="voxel-face is-front" />
          <i class="voxel-face is-back" />
          <i class="voxel-face is-right" />
          <i class="voxel-face is-left" />
          <i class="voxel-face is-top" />
          <i class="voxel-face is-bottom" />
        </div>
        <i class="voxel-satellite is-a" />
        <i class="voxel-satellite is-b" />
        <i class="voxel-satellite is-c" />
        <div class="voxel-shadow" />
      </div>
      <div class="hero-signature">
        <span class="hero-signature-mark">SNGL</span>
        <span class="hero-signature-line" />
        <span class="hero-signature-sub">CRAFT · EXPLORE · REPEAT</span>
      </div>
    </div>
  );
}

export default function Home(): JSX.Element {
  const ambientKey = useAmbientKey();
  const navigate = useNavigate();
  const [layout, setLayout] = createSignal<HeroLayout>(loadLayout());
  const [editing, setEditing] = createSignal(false);
  const [addOpen, setAddOpen] = createSignal(false);

  onMount(() => {
    void instancesStore.rescan();
    void authStore.refresh();
    void dashboard.listWidgets().then((res) => {
      if (res.ok) setCatalog(res.data);
      else toast({ message: res.error.message, tone: "error" });
    });
  });
  // Copied before sort: `.sort()` on the store's array would mutate it in
  // place (perf audit 2.4); memoized so it recomputes only on change.
  const sortedByLastPlayed = createMemo(() =>
    [...(instancesStore.items() ?? [])].sort((a, b) =>
      (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")
    )
  );

  // 默认选中最近游玩的实例；切换游戏目录后旧 key 不属于当前目录时也自动重选。
  // Effects, not memos, own side effects (perf audit 2.8).
  createEffect(() => {
    const items = instancesStore.items() ?? [];
    if (items.length === 0) return;
    const key = instancesStore.selectedKey();
    if (key && items.some((item) => item.summary.key === key)) return;
    const latest = [...items].sort((a, b) =>
      (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")
    )[0];
    if (latest) instancesStore.selectInstance(latest.summary.key);
  });

  const selected = () => instancesStore.selected() ?? null;
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

  function cardIcon(id: string): IconName {
    if (id.startsWith(pluginPrefix)) return "extension";
    const icons: Record<BuiltinCardId, IconName> = {
      server: "activity",
      compare: "refresh",
      announcements: "info",
      instances: "gamepad",
      versions: "package",
      downloads: "download",
      games: "terminal",
    };
    return icons[id as BuiltinCardId] ?? "package";
  }

  const addableCards = createMemo(() => {
    const current = layout();
    const builtins = HERO_DEFAULTS.order
      .filter((id) => !current.order.includes(id) || current.hidden.includes(id))
      .map((id) => ({ id, name: cardTitle(id), subtitle: "SNGL" }));
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
    const result = await launchStore.start(profile.summary.key, profile.summary.display_name);
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
    // B3-1 salvage: official accounts get the skin page shortcut back.
    ...(session()?.provider === "microsoft"
      ? [{
          label: t("home.account.skin"),
          icon: "external" as const,
          onSelect: () => void external.openUrl("https://www.minecraft.net/msaprofile/mygames/editskin"),
        }]
      : []),
    { label: t("accounts.manage"), icon: "settings" as const, onSelect: () => navigate("/accounts") },
    ...(session()
      ? [{ label: t("accounts.logout"), icon: "log-out" as const, danger: true, onSelect: () => void authStore.logout() }]
      : []),
  ];

  return (
    <div class="hero">
      {/* 主题 v2：Hero 背景图/视频 + 主界面特效（ADR-014） */}
      <ThemeHeroLayer />
      {/* 流动背景光斑 + 主题化环境粒子（随外观主题切换意象与配色；动效开关闸门） */}
      <div class={`hero-flow ${ambientThemeClass(ambientKey())}`} aria-hidden="true">
        <ThemeAmbient themeKey={ambientKey()} />
      </div>
      <VoxelScene />

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
              <div class="hero-module-heading">
                <span class="hero-module-heading-icon"><Icon name={cardIcon(id)} size={14} /></span>
                <span>{cardTitle(id)}</span>
                <i />
              </div>
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
                <Switch
                  fallback={
                    // B3-1 salvage: a crashed plugin card degrades to a
                    // retry/hide panel instead of taking the page down.
                    <ErrorBoundary
                      fallback={(err, reset) => (
                        <div class="hero-plugin-error">
                          <RmEmptyState icon="alert" title={t("home.card.broken")} hint={String(err)} />
                          <div class="btn-row">
                            <RmButton size="sm" onClick={reset}>{t("common.retry")}</RmButton>
                            <RmButton size="sm" variant="danger" onClick={() => hideCard(id)}>
                              {t("home.card.hideBroken")}
                            </RmButton>
                          </div>
                        </div>
                      )}
                    >
                      <PluginCard pluginId={id.slice(pluginPrefix.length)} />
                    </ErrorBoundary>
                  }
                >
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
                    <RmTag tone="accent">{providerLabel(current().provider)}</RmTag>
                    <span class="hero-account-meta-item">
                      {authStore.lastPlayedFor(current().account_id)
                        ? t("home.account.lastPlayed", {
                            time: relativeTime(authStore.lastPlayedFor(current().account_id)),
                          })
                        : t("instances.neverPlayed")}
                    </span>
                  </span>
                </span>
                <span class="hero-card-plus"><Icon name="chevron-down" size={14} /></span>
              </button>
            </RmMenu>
          )}
        </Show>

        {/** 运行状态卡：折叠时 Account 卡归位，展开时 Account 上移让位（CSS 动画） */}
        <div class={`hero-run-slot${launchStore.isRunning(selected()?.summary.key ?? "") ? " is-active" : ""}`}>
          <div class="hero-card hero-module hero-run-card">
            <span class="dot-running" />
            <span class="hero-run-label">{t("instances.running")}</span>
            <RmButton size="sm" icon="terminal" onClick={() => uiStore.openLogDrawer(selected()?.summary.key ?? null)}>
              {t("home.viewLog")}
            </RmButton>
            {/* 一键强制关闭：terminate_tree 整组终止，经 stop() 统一确认 */}
            <RmButton
              size="sm"
              variant="danger"
              icon="square"
              onClick={() => void launchStore.stop(selected()?.summary.key ?? "")}
            >
              {t("home.forceStop")}
            </RmButton>
          </div>
        </div>

        <div class="hero-card hero-module hero-launch">
          <div class="hero-launch-main">
            {/* B3-1 salvage: no instances yet → guide CTA instead of a dead
                launch button (delegates to the install wizard). */}
            <Show
              when={(instancesStore.items() ?? []).length > 0}
              fallback={
                <div class="hero-launch-guide">
                  <RmButton size="lg" variant="primary" icon="download" onClick={() => navigate("/download#install")}>
                    {t("home.noInstance.cta")}
                  </RmButton>
                  <span class="muted-line">{t("home.noInstance.hint")}</span>
                </div>
              }
            >
              <RmMenu
                align="left"
                side="up"
                items={[
                  ...sortedByLastPlayed().map((p) => ({
                      label: `${p.summary.display_name} · ${p.summary.game_version}`,
                      icon: "gamepad" as const,
                      onSelect: () => instancesStore.selectInstance(p.summary.key),
                    })),
                  { label: t("download.wizard.bindLocal"), icon: "download" as const, onSelect: () => navigate("/download#install") },
                  ...(selected()
                    ? [{ label: t("home.instanceSettings"), icon: "sliders" as const, onSelect: () => navigate(`/instances/${selected()!.summary.key}#settings`) }]
                    : []),
                ]}
              >
                <button class="hero-launch-picker" title={t("home.pickInstance")}>
                  <span class="hero-launch-picker-copy">
                    <span class="hero-launch-picker-main">
                      {selected()?.summary.display_name ?? t("home.pickInstance")}
                    </span>
                    <Show when={selected()}>
                      <span class="hero-launch-picker-sub">
                        {selected()!.summary.play_time_seconds > 0
                          ? t("playtime.cumulative", {
                              time: playtimeLabel(Number(selected()!.summary.play_time_seconds)),
                            })
                          : t("playtime.none")}
                      </span>
                    </Show>
                  </span>
                  <Icon class="hero-launch-picker-caret" name="chevron-up" size={15} />
                </button>
              </RmMenu>
              <RmButton
                variant="primary"
                icon="play"
                size="lg"
                loading={launchStore.isStarting(selected()?.summary.key ?? "")}
                onClick={() => void launchCurrent()}
              >
                {t("home.launch")}
              </RmButton>
            </Show>
          </div>
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
  const clientVersion = () => instancesStore.selected()?.summary.game_version ?? "26.3";
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
  const navigate = useNavigate();
  const directory = () => instancesStore.selectedDirectory();
  return (
    <div class="hero-list">
      <Show when={directory()}>
        <button class="hero-list-dir" onClick={() => navigate("/instances")}>
          <Icon name="folder" size={12} />
          <span>{directory()!.directory.name}</span>
          <span class="hero-list-dir-count">{directory()!.instance_count}</span>
        </button>
      </Show>
      <For each={instancesStore.items() ?? []}>
        {(profile) => (
          <button
            class={`hero-row${(instancesStore.selected()?.summary.key ?? null) === profile.summary.key ? " is-selected" : ""}`}
            onClick={() => instancesStore.selectInstance(profile.summary.key)}
          >
            <span class="hero-row-icon"><Icon name="gamepad" size={16} /></span>
            <span class="hero-row-main">
              <span class="hero-row-title">{profile.summary.display_name}</span>
              <span class="hero-row-subtitle">
                {profile.summary.game_version}
                <Show when={profile.summary.loader}> · {loaderLabel(profile.summary.loader!.kind)}</Show>
              </span>
            </span>
            <span class="hero-row-meta">
              {profile.summary.last_played ? relativeTime(profile.summary.last_played) : t("instances.neverPlayed")}
            </span>
          </button>
        )}
      </For>
      <Show when={(instancesStore.items() ?? []).length === 0}>
        <div class="hero-empty">{t("instances.empty.title")}</div>
      </Show>
    </div>
  );
}

function VersionsCard(): JSX.Element {
  const navigate = useNavigate();
  const [versions_, setVersions_] = createSignal<VersionInfo[] | null>(null);
  // B3-1 salvage: paste any version id and jump into the wizard with it
  // preselected (query on the path, hash carries the tab — P0-1 format).
  const [pasted, setPasted] = createSignal("");
  onMount(() => {
    // versionCache keeps this card instant and offline-friendly (N-19).
    setVersions_(
      versionCache.gameVersionsWithRefresh()?.filter((v) => v.release_type === "release") ?? null
    );
    void versionCache.refreshGameVersions().then((list) => {
      if (list) setVersions_(list.filter((v) => v.release_type === "release"));
    });
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
          <button class="hero-row" onClick={() => navigate(`/download?version=${version.id}#install`)}>
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
      <div class="hero-paste-row">
        <RmInput
          value={pasted()}
          onInput={setPasted}
          placeholder={t("home.pasteVersion")}
          onEnter={() => pasted().trim() && navigate(`/download?version=${pasted().trim()}#install`)}
        />
      </div>
    </div>
  );
}

function DownloadsCard(): JSX.Element {
  const batches = createMemo(() => Object.values(downloadsStore.batches) as DownloadBatch[]);
  // B3-1 salvage: per-row cancel (same confirm flow as the task center).
  async function cancelBatch(batchId: string): Promise<void> {
    const allowed = await confirmDanger({
      title: t("tasks.cancelConfirm.title"),
      body: t("tasks.cancelConfirm.body"),
      confirmLabel: t("tasks.download.cancel"),
    });
    if (!allowed) return;
    const res = await downloads.cancel(batchId);
    toast({
      message: res.ok ? t("tasks.download.cancelled") : res.error.message,
      tone: res.ok ? "info" : "error",
    });
  }
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
            <RmIconButton
              icon="x"
              label={t("tasks.download.cancel")}
              danger
              onClick={() => void cancelBatch(batch.batchId)}
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
                {instancesStore.items()?.find((p) => p.summary.key === game.profileId)?.summary.display_name ?? game.profileId}
              </span>
              <span class="hero-row-subtitle">{t("instances.running")}</span>
            </span>
            <RmIconButton icon="terminal" label={t("home.viewLog")} onClick={() => uiStore.openLogDrawer(game.profileId)} />
            <RmIconButton icon="square" label={t("common.stop")} danger onClick={() => void launchStore.stop(game.profileId)} />
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
