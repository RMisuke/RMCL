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

  const lines = createMemo(() => launchStore.buffers().get(profileId() ?? "") ?? []);
  const filtered = createMemo(() =>
    lines().filter((line) => {
      if (filter() && !line.toLowerCase().includes(filter().toLowerCase())) return false;
      const lvl = levelOf(line);
      if (level() === "error") return lvl === "error";
      if (level() === "warn") return lvl === "warn" || lvl === "error";
      return true;
    })
  );

  const exit = createMemo(() => {
    const last = launchStore.lastExit();
    return last && last.profileId === profileId() ? last : null;
  });

  const analysis = () => aiStore.analysis();
  const aiConfigured = () => aiStore.config()?.configured ?? false;
  const [egressPrompt, setEgressPrompt] = createSignal(false);

  createEffect(() => {
    if (autoscroll() && scrollBox) {
      scrollBox.scrollTop = scrollBox.scrollHeight;
    }
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
              </div>
              <div class="ai-panel-text">
                <Show
                  when={state().state !== "error"}
                  fallback={<div class="ai-panel-error">{state().error}</div>}
                >
                  {state().text}
                  <Show when={state().state === "running"}>
                    <span class="ai-cursor">▌</span>
                  </Show>
                </Show>
              </div>
              <Show when={state().state === "done"}>
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
