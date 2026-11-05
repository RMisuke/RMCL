/**
 * Bottom-right launch capsule.
 *
 * The left segment shows the instance name (truncated) above the account-wide
 * play time; the chevron opens the instance/version list. The right segment is
 * a fixed-width launch button whose icon + label stay optically centred.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { Icon } from "./icons";
import { GrassBlock } from "./GrassBlock";
import { instancesStore } from "../stores/instances";
import { launchStore } from "../stores/launch";
import { launchInstance } from "../lib/launchActions";
import { t } from "../i18n";
import { uiStore } from "../stores/ui";
import { loaderLabel } from "../lib/loaders";
import { playtimeLabel } from "../lib/playtime";
import type { InstanceDetail, InstanceIcon } from "../ipc/types";

function instanceIconUrl(icon: InstanceIcon): string | null {
  if (typeof icon === "object" && icon !== null && "type" in icon) {
    if (icon.type === "data_url" || icon.type === "file") {
      return String((icon as { value: string }).value);
    }
  }
  return null;
}

function VersionGlyph(props: { item: InstanceDetail }): JSX.Element {
  const url = () => instanceIconUrl(props.item.summary.icon);
  return (
    <span class={`launch-version-glyph${props.item.summary.modded ? " is-modded" : ""}`}>
      <Show when={url()} fallback={<GrassBlock small />}>
        <img src={url()!} alt="" />
      </Show>
    </span>
  );
}

export function LaunchDock(): JSX.Element {
  const navigate = useNavigate();
  const [open, setOpen] = createSignal(false);
  let root: HTMLDivElement | undefined;

  const versions = createMemo(() =>
    [...(instancesStore.items() ?? [])].sort((a, b) =>
      (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "")
    )
  );
  const current = () => instancesStore.selected() ?? versions()[0] ?? null;
  const running = () => (current() ? launchStore.isRunning(current()!.summary.key) : false);
  const starting = () => (current() ? launchStore.isStarting(current()!.summary.key) : false);
  const totalPlaytime = createMemo(() =>
    (instancesStore.items() ?? []).reduce(
      (sum, item) => sum + Number(item.summary.play_time_seconds ?? 0),
      0
    )
  );

  const mainLabel = () => {
    if (starting()) return t("dock.launch.launching");
    return current() ? t("dock.launch.game") : t("home.noInstance.cta");
  };
  const instanceName = () => current()?.summary.display_name ?? t("dock.launch.empty");
  const playtime = () =>
    totalPlaytime() > 0
      ? t("playtime.cumulative", { time: playtimeLabel(totalPlaytime()) })
      : t("playtime.none");

  const onDocDown = (event: MouseEvent) => {
    if (open() && root && !root.contains(event.target as Node)) setOpen(false);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && open()) setOpen(false);
  };
  onMount(() => {
    document.addEventListener("mousedown", onDocDown);
    document.addEventListener("keydown", onKeyDown);
  });
  onCleanup(() => {
    document.removeEventListener("mousedown", onDocDown);
    document.removeEventListener("keydown", onKeyDown);
  });

  async function onMain(): Promise<void> {
    const profile = current();
    if (!profile) {
      navigate("/download#install");
      return;
    }
    // A running game is stopped from the dedicated run card above the capsule.
    if (starting() || running()) return;
    await launchInstance(profile, navigate);
  }

  async function choose(key: string): Promise<void> {
    await instancesStore.selectInstance(key);
    setOpen(false);
  }

  function go(href: string): void {
    setOpen(false);
    navigate(href);
  }

  return (
    <div
      class={`launch-dock${running() ? " is-running" : ""}${starting() ? " is-starting" : ""}`}
      ref={root}
    >
      <Show when={open()}>
        <div class="launch-popover" role="listbox" aria-label={t("home.pickInstance")}>
          <div class="launch-popover-head">
            <span>{t("home.pickInstance")}</span>
            <span>{versions().length}</span>
          </div>
          <For each={versions()}>
            {(item) => (
              <button
                class={`launch-version-row${item.summary.key === current()?.summary.key ? " is-active" : ""}`}
                role="option"
                aria-selected={item.summary.key === current()?.summary.key}
                onClick={() => void choose(item.summary.key)}
              >
                <VersionGlyph item={item} />
                <span class="launch-version-copy">
                  <strong>{item.summary.display_name}</strong>
                  <small>
                    {item.summary.game_version}
                    <Show when={item.summary.loader}>
                      {" · "}
                      {loaderLabel(item.summary.loader!.kind)}
                    </Show>
                  </small>
                </span>
                <Show when={item.summary.key === current()?.summary.key}>
                  <Icon name="check" size={15} />
                </Show>
              </button>
            )}
          </For>
          <Show when={versions().length === 0}>
            <div class="launch-popover-empty">{t("dock.launch.empty")}</div>
          </Show>
          <div class="launch-popover-sep" />
          <button class="launch-popover-action" role="option" onClick={() => go("/download#install")}>
            <Icon name="plus" size={15} />
            {t("home.noInstance.cta")}
          </button>
          <button class="launch-popover-action" role="option" onClick={() => go("/instances")}>
            <Icon name="grid" size={15} />
            {t("dock.launch.manage")}
          </button>
        </div>
      </Show>

      <Show when={running() && current()}>
        {(profile) => (
          <div class="launch-run-card">
            <button
              class="launch-run-action"
              onClick={() => uiStore.openLogDrawer(profile().summary.key)}
            >
              <Icon name="terminal" size={16} />
              {t("home.viewLog")}
            </button>
            <button
              class="launch-run-action is-danger"
              onClick={() => void launchStore.stop(profile().summary.key)}
            >
              <Icon name="square" size={15} />
              {t("dock.launch.forceStop")}
            </button>
          </div>
        )}
      </Show>

      <div class="launch-capsule">
        <button
          class={`launch-capsule-info${open() ? " is-open" : ""}`}
          aria-haspopup="listbox"
          aria-expanded={open()}
          aria-label={t("home.pickInstance")}
          onClick={() => setOpen(!open())}
        >
          <span class="launch-capsule-copy">
            <strong title={instanceName()}>{instanceName()}</strong>
            <small>{playtime()}</small>
          </span>
          <Icon name="chevron-up" size={17} class="launch-capsule-arrow" />
        </button>
        <button
          class="launch-capsule-start"
          disabled={starting()}
          onClick={() => void onMain()}
          title={current()?.summary.display_name ?? t("home.noInstance.hint")}
        >
          <Show when={starting()} fallback={<Icon name={current() ? "play" : "download"} size={18} />}>
            <span class="launch-capsule-spinner" />
          </Show>
          <span>{mainLabel()}</span>
        </button>
      </div>
    </div>
  );
}
