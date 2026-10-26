/**
 * Game log drawer (ui-design 10 §4): right-side slide-in with per-profile
 * buffers, level coloring + filter, and the streaming AI-analysis panel
 * (ADR-013 S2 — the built-in reference consumer).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onCleanup } from "solid-js";
import { Icon } from "./icons";
import { RmButton, RmIconButton, RmInput, RmTag } from "./rm/controls";
import { launchStore } from "../stores/launch";
import { profilesStore } from "../stores/profiles";
import { aiStore } from "../stores/ai";
import { uiStore, toast } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { describeExit } from "../lib/exitCodes";
import { renderMarkdown } from "../lib/markdown";

type LevelFilter = "all" | "info" | "warn" | "error";

function levelOf(line: string): "error" | "warn" | "info" {
  if (line.includes("ERROR") || line.includes("FATAL")) return "error";
  if (line.includes("WARN")) return "warn";
  return "info";
}

export function LogDrawer(): JSX.Element {
  const drawer = () => uiStore.logDrawer();
  const profileId = () => drawer()?.profileId ?? null;
  const profileName = createMemo(
    () => profilesStore.items()?.find((p) => p.id === profileId())?.name ?? t("logs.sharedDir")
  );
  const [filter, setFilter] = createSignal("");
  const [level, setLevel] = createSignal<LevelFilter>("all");
  const [autoscroll, setAutoscroll] = createSignal(true);
  let scrollBox: HTMLDivElement | undefined;

  // Lazy subscriptions (perf audit 1.2): both memos gate on `drawer()` and
  // return early while the drawer is hidden, so Solid drops the `buffers`
  // dependency entirely and the 1000-line filter does not re-run per log
  // batch while the drawer is closed (it is mounted app-wide in App.tsx).
  const lines = createMemo(() =>
    drawer() ? (launchStore.buffers().get(profileId() ?? "") ?? []) : []
  );
  const filtered = createMemo(() => {
    if (!drawer()) return [];
    return lines().filter((line) => {
      if (filter() && !line.toLowerCase().includes(filter().toLowerCase())) return false;
      const lvl = levelOf(line);
      if (level() === "error") return lvl === "error";
      if (level() === "warn") return lvl === "warn" || lvl === "error";
      return true;
    });
  });

  const exit = createMemo(() => {
    const last = launchStore.lastExit();
    return last && last.profileId === profileId() ? last : null;
  });

  const analysis = () => aiStore.analysis();
  const aiConfigured = () => aiStore.config()?.configured ?? false;
  const [egressPrompt, setEgressPrompt] = createSignal(false);

  // AI 分析展开窗（左侧独立面板，可拖宽度；默认 46% 宽，持久化）。
  const [aiExpanded, setAiExpanded] = createSignal(false);
  const AI_WIDTH_KEY = "rmccc.aiPaneWidth";
  const initialAiWidth = (() => {
    const stored = Number(localStorage.getItem(AI_WIDTH_KEY));
    return Number.isFinite(stored) && stored >= 320 ? stored : Math.round(window.innerWidth * 0.46);
  })();
  const [aiWidth, setAiWidth] = createSignal(initialAiWidth);
  let aiWindowEl: HTMLDivElement | undefined;

  function startResize(e: PointerEvent): void {
    const startX = e.clientX;
    const startWidth = aiWidth();
    const onMove = (ev: PointerEvent) => {
      const next = startWidth + (ev.clientX - startX);
      const clamped = Math.max(320, Math.min(next, window.innerWidth - 80));
      setAiWidth(clamped);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      try {
        localStorage.setItem(AI_WIDTH_KEY, String(aiWidth()));
      } catch {
        /* 持久化失败仅影响下次默认宽度 */
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    // P3-9: if the drawer unmounts mid-drag the listeners must go too.
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    });
  }

  // Autoscroll via rAF (perf audit 2.2): reading `scrollHeight` forces a
  // layout; coalesce bursts of log batches into one read per frame.
  let autoscrollRaf = 0;
  createEffect(() => {
    if (autoscroll() && scrollBox) {
      if (!autoscrollRaf) {
        autoscrollRaf = requestAnimationFrame(() => {
          autoscrollRaf = 0;
          if (autoscroll() && scrollBox) {
            scrollBox.scrollTop = scrollBox.scrollHeight;
          }
        });
      }
    }
  });
  onCleanup(() => {
    if (autoscrollRaf) cancelAnimationFrame(autoscrollRaf);
  });

  // Keep the buffer alive while the drawer is open; drop it on close
  // unless a crash analysis may still be wanted.
  onCleanup(() => {
    const id = profileId();
    if (id && !exit()) launchStore.dropBuffer(id);
  });

  async function startAnalysis(): Promise<void> {
    if (!aiConfigured()) {
      toast({ message: t("logs.ai.notConfigured"), tone: "info" });
      return;
    }
    if (!aiStore.egressApproved()) {
      setEgressPrompt(true);
      return;
    }
    const res = await aiStore.analyzeLog(profileId());
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  return (
    <Show when={drawer()}>
      <aside class="log-drawer" role="complementary">
        <header class="log-drawer-head">
          <h3>
            {t("logs.title", { name: profileName() })}
          </h3>
          <div class="log-drawer-actions">
            <RmButton
              size="sm"
              icon="bot"
              disabled={!aiConfigured() || analysis()?.state === "running"}
              title={aiConfigured() ? undefined : t("logs.ai.notConfigured")}
              onClick={() => void startAnalysis()}
            >
              {analysis()?.state === "running" ? t("logs.ai.analyzing") : t("logs.ai.analyze")}
            </RmButton>
            <RmIconButton
              icon={autoscroll() ? "chevron-down" : "chevron-right"}
              label={t("logs.autoscroll")}
              onClick={() => setAutoscroll(!autoscroll())}
            />
            <RmIconButton
              icon="copy"
              label={t("common.copy")}
              onClick={() => {
                void navigator.clipboard.writeText(lines().join("\n"));
                toast({ message: t("logs.copied"), tone: "success" });
              }}
            />
            <RmIconButton
              icon="x"
              label={t("common.close")}
              onClick={() => {
                const id = profileId();
                if (id && exit()) launchStore.keepBuffer(id);
                uiStore.closeLogDrawer();
              }}
            />
          </div>
        </header>

        <Show when={exit()}>
          {(exitInfo) => {
            const desc = describeExit(exitInfo().exitCode);
            return (
              <div class={`log-exit-banner is-${desc.severity}`}>
                {t(desc.key as TranslationKey, desc.params)}
              </div>
            );
          }}
        </Show>

        <div class="log-filter-bar">
          <RmInput value={filter()} onInput={setFilter} placeholder={t("logs.filter")} />
          <div class="log-level-chips">
            <For each={["all", "info", "warn", "error"] as const}>
              {(lvl) => (
                <button
                  class={`rm-chip${level() === lvl ? " is-active" : ""}`}
                  onClick={() => setLevel(lvl)}
                >
                  {t(`logs.level.${lvl}`)}
                </button>
              )}
            </For>
          </div>
        </div>

        <div class="log-lines" ref={scrollBox}>
          <For each={filtered()}>
            {(line) => <div class={`log-line log-${levelOf(line)}`}>{line}</div>}
          </For>
          <Show when={filtered().length === 0}>
            <div class="log-empty">{t("logs.empty")}</div>
          </Show>
        </div>

        <Show when={analysis()}>
          {(state) => (
            <div class="ai-panel">
              <div class="ai-panel-head">
                <Icon name="bot" size={14} />
                <span>
                  {state().state === "running"
                    ? t("logs.ai.analyzing")
                    : state().state === "error"
                      ? t("logs.ai.failed")
                      : t("logs.ai.doneTitle")}
                </span>
                <span class="ai-panel-meta">
                  {t("logs.ai.sentNote", { lines: String(state().sentLines) })}
                </span>
                <Show when={state().state === "running"}>
                  <RmButton size="sm" variant="ghost" onClick={() => void aiStore.cancelAnalysis()}>
                    {t("common.cancel")}
                  </RmButton>
                </Show>
                <RmButton
                  size="sm"
                  variant="ghost"
                  icon={aiExpanded() ? "chevron-down" : "chevron-right"}
                  onClick={() => setAiExpanded(!aiExpanded())}
                >
                  {aiExpanded() ? t("logs.ai.collapse") : t("logs.ai.expand")}
                </RmButton>
              </div>
              <Show when={!aiExpanded()}>
                <div class="ai-panel-text">
                  <Show
                    when={state().state !== "error"}
                    fallback={<div class="ai-panel-error">{state().error}</div>}
                  >
                    {/* 先转义后解析的安全 Markdown 渲染（lib/markdown），innerHTML 不可注入 */}
                    <div class="ai-md" innerHTML={renderMarkdown(state().text)} />
                    <Show when={state().state === "running"}>
                      <span class="ai-cursor">▌</span>
                    </Show>
                  </Show>
                </div>
              </Show>
              <Show when={!aiExpanded() && state().state === "done"}>
                <div class="ai-panel-actions">
                  <RmButton
                    size="sm"
                    icon="copy"
                    onClick={() => {
                      void navigator.clipboard.writeText(state().text);
                      toast({ message: t("logs.copied"), tone: "success" });
                    }}
                  >
                    {t("logs.ai.copy")}
                  </RmButton>
                  <RmButton size="sm" icon="refresh" onClick={() => void startAnalysis()}>
                    {t("logs.ai.retry")}
                  </RmButton>
                </div>
              </Show>
            </div>
          )}
        </Show>

        {/* AI 分析展开窗：左侧大面板（可拖右缘调宽），日志抽屉保持在右侧 */}
        <Show when={aiExpanded() && analysis()}>
          {(s) => {
            // 注意：必须经 Show 回调参数 s() 访问最新 analysis（Solid 的
            // Show 回调体在 untrack 中执行，`analysis() && aiExpanded()` 形式
            // 的布尔 when + 快照解构会拿不到流式更新——点开窗后文字冻结，
            // 收回才显示（2026-10-25 实测）。
            return (
              <div class="ai-window" ref={aiWindowEl} style={{ width: `${aiWidth()}px` }}>
                <div class="ai-window-resize" onPointerDown={(e) => startResize(e)} />
                <header class="ai-window-head">
                  <Icon name="bot" size={16} />
                  <span class="ai-window-title">
                    {s().state === "running"
                      ? t("logs.ai.analyzing")
                      : s().state === "error"
                        ? t("logs.ai.failed")
                        : t("logs.ai.doneTitle")}
                  </span>
                  <span class="ai-panel-meta">
                    {t("logs.ai.sentNote", { lines: String(s().sentLines) })}
                  </span>
                  <div class="btn-row">
                    <RmButton
                      size="sm"
                      icon="copy"
                      onClick={() => {
                        void navigator.clipboard.writeText(s().text);
                        toast({ message: t("logs.copied"), tone: "success" });
                      }}
                    >
                      {t("logs.ai.copy")}
                    </RmButton>
                    <RmButton size="sm" icon="refresh" onClick={() => void startAnalysis()}>
                      {t("logs.ai.retry")}
                    </RmButton>
                    <Show when={s().state === "running"}>
                      <RmButton size="sm" variant="ghost" onClick={() => void aiStore.cancelAnalysis()}>
                        {t("common.cancel")}
                      </RmButton>
                    </Show>
                    <RmIconButton
                      icon="x"
                      label={t("logs.ai.collapse")}
                      onClick={() => setAiExpanded(false)}
                    />
                  </div>
                </header>
                <div class="ai-window-body">
                  <Show
                    when={s().state !== "error"}
                    fallback={<div class="ai-panel-error">{s().error}</div>}
                  >
                    <div class="ai-md" innerHTML={renderMarkdown(s().text)} />
                    <Show when={s().state === "running"}>
                      <span class="ai-cursor">▌</span>
                    </Show>
                  </Show>
                </div>
              </div>
            );
          }}
        </Show>

        <Show when={egressPrompt()}>
          <div class="egress-prompt">
            <div>{t("logs.ai.egressBody")}</div>
            <div class="egress-prompt-actions">
              <RmButton
                size="sm"
                onClick={() => setEgressPrompt(false)}
              >
                {t("common.cancel")}
              </RmButton>
              <RmButton
                size="sm"
                variant="primary"
                onClick={() => {
                  aiStore.approveEgress();
                  setEgressPrompt(false);
                  void startAnalysis();
                }}
              >
                {t("logs.ai.egressAllow")}
              </RmButton>
            </div>
          </div>
        </Show>
      </aside>
    </Show>
  );
}
