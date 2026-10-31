/**
 * Launch bubble (HMCL launching-steps style): bottom-right progress bubble
 * that follows the backend's real `launch://progress` events. It stays in
 * "waiting for game window" until stdout proves the window is ready; process
 * spawn alone is never shown as 100%.
 */
import { Show, createMemo } from "solid-js";
import type { JSX } from "solid-js";
import { RmIconButton, RmProgress } from "./rm/controls";
import { launchStore } from "../stores/launch";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";

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
      case "waiting":
        return "launchFlow.stage.waiting";
      case "started":
        return "launchFlow.stage.started";
      case "failed":
        return "launchFlow.stage.failed";
      default:
        return null;
    }
  });

  const detail = createMemo(() => {
    const f = flow();
    if (!f) return "";
    if (f.stage === "failed") return f.error ?? f.message ?? "";
    return f.message ?? "";
  });

  const indeterminate = () => {
    const f = flow();
    return !!f && f.stage !== "started" && f.stage !== "failed" && f.percent <= 0;
  };

  return (
    <Show when={flow()}>
      {(f) => (
        <div class={`launch-bubble${f().stage === "failed" ? " is-failed" : ""}`} role="status">
          <div class="launch-bubble-head">
            <span class="launch-bubble-title">{t("launchFlow.title", { name: f().profileName })}</span>
            <span class="launch-bubble-percent">{Math.min(100, Math.max(0, Math.round(f().percent)))}%</span>
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
          <RmProgress value={f().percent} determinate={!indeterminate()} />
          <Show when={detail()}>
            <div class="launch-bubble-detail">{detail()}</div>
          </Show>
        </div>
      )}
    </Show>
  );
}
