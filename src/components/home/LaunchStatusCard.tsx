/**
 * Hero launch-status card.
 *
 * The percentage is sourced from `launch://progress` only. `100%` has one
 * meaning: the backend observed a game-window readiness marker in the game's
 * stdout. Process spawn (`LAUNCH_STARTED`) tops out at 88%.
 */
import { Show, createMemo } from "solid-js";
import type { JSX } from "solid-js";
import { Icon } from "../icons";
import { RmProgress } from "../rm/controls";
import { providerLabel } from "../../lib/format";
import { t } from "../../i18n";
import { authStore } from "../../stores/auth";
import { instancesStore } from "../../stores/instances";
import { launchStore } from "../../stores/launch";

export function LaunchStatusCard(): JSX.Element {
  const selected = () => instancesStore.selected() ?? null;
  const session = () => authStore.session();
  const key = () => selected()?.summary.key ?? "";
  const running = () => launchStore.isRunning(key());
  const starting = () => launchStore.isStarting(key());
  const progress = () => (key() ? launchStore.progressFor(key()) : null);
  const flow = createMemo(() => {
    const current = launchStore.flow();
    return current && current.profileId === key() ? current : null;
  });

  const percent = createMemo(() => {
    if (!selected()) return 0;
    const p = progress();
    if (p) return p.windowOpened ? 100 : Math.min(99, Math.round(p.percent));
    if (running()) return 100;
    return 0;
  });

  const title = createMemo(() => {
    if (!selected()) return t("launch.status.noInstance");
    const p = progress();
    if (flow()?.stage === "failed") return t("launch.status.failed");
    if (p?.windowOpened) return t("launch.status.windowOpen");
    switch (p?.stage) {
      case "checking":
        return t("launch.status.checking");
      case "repairing":
        return t("launch.status.repairing");
      case "checking-java":
        return t("launch.status.java");
      case "spawning":
        return t("launch.status.spawning");
      case "waiting-window":
        return t("launch.status.waiting");
      case "window-open":
        return t("launch.status.windowOpen");
      default:
        if (starting()) return t("launch.status.checking");
        if (running()) return t("launch.status.running");
        return t("launch.status.ready");
    }
  });

  const message = createMemo(() => {
    if (!selected()) return t("launch.status.noInstanceHint");
    if (flow()?.stage === "failed") {
      return flow()?.error ?? flow()?.message ?? t("launch.status.failed");
    }
    if (percent() >= 100) return t("launch.status.windowOpenHint");
    return progress()?.message ?? t("launch.status.readyHint");
  });

  const tone = () => {
    if (flow()?.stage === "failed") return " is-failed";
    if (percent() >= 100) return " is-ready";
    if (starting() || progress()) return " is-active";
    return "";
  };

  return (
    <article class={`soft-card soft-status-card${tone()}`}>
      <div class="soft-status-ring" style={{ "--ready": `${percent() * 3.6}deg` }}>
        <div>
          <strong>{percent()}<small>%</small></strong>
          <span>{percent() >= 100 ? t("launch.status.windowShort") : t("soft.launchStatus")}</span>
        </div>
      </div>
      <div class="soft-status-copy">
        <div class="soft-card-heading">
          <span>{t("soft.launchStatus")}</span>
          <Icon name={flow()?.stage === "failed" ? "alert" : "activity"} size={15} />
        </div>
        <strong>{title()}</strong>
        <span class="soft-status-message">{message()}</span>
        <RmProgress value={percent()} determinate={percent() > 0} />
        <Show when={selected()}>
          <div class="soft-status-legend">
            <span title={selected()!.summary.key}>
              <i class={running() ? "is-green" : "is-blue"} />
              {selected()!.summary.display_name}
            </span>
            <span>
              <i class="is-blue" />
              {session() ? providerLabel(session()!.provider) : t("home.account.add")}
            </span>
          </div>
        </Show>
      </div>
    </article>
  );
}
