/**
 * Bottom-right launch dock (soft-skeuo dashboard).
 *
 * HMCL-style split button: the main area launches the current instance and the
 * arrow opens a glass version list. Styled with the same frosted dock shell as
 * the navigation bar; the primary area keeps the launcher's blue glass accent.
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
import { loaderLabel } from "../lib/loaders";
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

  const mainLabel = () => {
    if (running()) return t("dock.launch.stop");
    if (starting()) return t("dock.launch.launching");
    return current() ? t("dock.launch.game") : t("home.noInstance.cta");
  };

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
    if (starting()) return;
    if (running()) {
      await launchStore.stop(profile.summary.key);
      return;
    }
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
        <div class="dock-popover launch-popover" role="listbox" aria-label={t("home.pickInstance")}>
          <div class="dock-popover-head">
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
                <span class="dock-row-copy">
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
            <div class="dock-popover-empty">{t("dock.launch.empty")}</div>
          </Show>
          <div class="dock-popover-sep" />
          <button class="dock-action-row" role="option" onClick={() => go("/download#install")}>
            <Icon name="plus" size={15} />
            {t("home.noInstance.cta")}
          </button>
          <button class="dock-action-row" role="option" onClick={() => go("/instances")}>
            <Icon name="grid" size={15} />
            {t("dock.launch.manage")}
          </button>
        </div>
      </Show>

      <div class="launch-dock-body">
        <button
          class="launch-dock-main"
          disabled={starting()}
          onClick={() => void onMain()}
          title={current()?.summary.display_name ?? t("home.noInstance.hint")}
        >
          <span class="launch-dock-icon">
            <Show
              when={starting()}
              fallback={
                <Icon
                  name={running() ? "square" : current() ? "play" : "download"}
                  size={19}
                />
              }
            >
              <span class="launch-dock-spinner" />
            </Show>
          </span>
          <span class="launch-dock-copy">
            <strong>{mainLabel()}</strong>
            <small>{current()?.summary.display_name ?? t("dock.launch.choose")}</small>
          </span>
        </button>
        <button
          class={`launch-dock-arrow${open() ? " is-open" : ""}`}
          aria-haspopup="listbox"
          aria-expanded={open()}
          aria-label={t("home.pickInstance")}
          disabled={versions().length === 0}
          onClick={() => setOpen(!open())}
        >
          <Icon name="chevron-up" size={16} />
        </button>
      </div>
    </div>
  );
}
