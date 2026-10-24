/**
 * Launch bubble (HMCL launching-steps style): bottom-right progress bubble
 * that surfaces the launch pipeline — resource check → auto-repair → start →
 * "游戏已启动" — and closes itself after success.
 */
import { Show, createMemo } from "solid-js";
import type { JSX } from "solid-js";
import { RmIconButton, RmProgress } from "./rm/controls";
import { launchStore } from "../stores/launch";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";

const STAGE_PERCENT: Record<string, number> = {
  check: 12,
  repair: 45,
  launching: 78,
  started: 100,
  failed: 100,
};

export function LaunchBubble(): JSX.Element {
  const flow = () => launchStore.flow();

  const stageKey = createMemo<TranslationKey | null>(() => {
    const f = flow();
    if (!f) return null;
    switch (f.stage) {
      case "check":
        return "launchFlow.stage.check";
      case "repair":
        return "launchFlow.stage.repair";
      case "launching":
        return "launchFlow.stage.launching";
      case "started":
        return "launchFlow.stage.started";
      case "failed":
        return "launchFlow.stage.failed";
    }
  });

  const detail = createMemo(() => {
    const f = flow();
    if (!f) return "";
    if (f.stage === "failed") return f.error ?? "";
    if (f.stage === "repair" && f.missing !== undefined) {
      return t("launchFlow.detail.missing", { n: f.missing });
    }
    if (f.stage === "launching" && f.repaired !== undefined && f.repaired > 0) {
      return t("launchFlow.detail.repaired", { n: f.repaired });
    }
    return "";
  });

  const indeterminate = () => {
    const f = flow();
    return !!f && (f.stage === "check" || f.stage === "launching");
  };

  return (
    <Show when={flow()}>
      {(f) => (
        <div class={`launch-bubble${f().stage === "failed" ? " is-failed" : ""}`} role="status">
          <div class="launch-bubble-head">
            <span class="launch-bubble-title">{t("launchFlow.title", { name: f().profileName })}</span>
            <RmIconButton icon="x" label={t("common.close")} onClick={() => launchStore.dismissFlow()} />
          </div>
          <Show when={stageKey()}>
            {(key) => (
              <div class={`launch-bubble-stage${f().stage === "started" ? " is-ok" : ""}`}>
                <Show when={f().stage !== "started"} fallback={<span class="launch-bubble-check">✓</span>}>
                  <span class="launch-bubble-spinner" />
                </Show>
                {t(key())}
              </div>
            )}
          </Show>
          <RmProgress
            value={STAGE_PERCENT[f().stage] ?? 0}
            determinate={!indeterminate()}
          />
          <Show when={detail()}>
            <div class="launch-bubble-detail">{detail()}</div>
          </Show>
        </div>
      )}
    </Show>
  );
}
