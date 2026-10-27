/**
 * Instance list (ADR-016): a HMCL-style game-directory drawer on the left and
 * the selected directory's version-backed instances on the right.
 *
 * There is no separate "version repository" page — an instance *is*
 * `versions/{id}`. The list deliberately skips the expensive completeness scan;
 * health checks live in the instance detail maintenance tab.
 *
 * Card/list are two presentations of the same reactive list: the switch is a
 * `Switch`, not two mounted trees, so toggling never loses scroll state or
 * re-runs the directory query.
 */
import type { JSX } from "solid-js";
import { Show, For, Switch, Match, createMemo, createSignal, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { Icon } from "../components/icons";
import { RmButton, RmIconButton, RmInput, RmSelect, RmTag } from "../components/rm/controls";
import { RmMenu } from "../components/rm/overlays";
import type { MenuItem } from "../components/rm/overlays";
import { RmContentState, RmEmptyState, RmErrorState } from "../components/rm/surfaces";
import { instancesStore } from "../stores/instances";
import { launchStore } from "../stores/launch";
import { instances, dialog } from "../ipc/api";
import type { GameDirectoryOverview, InstanceDetail } from "../ipc/types";
import type { DeleteInstanceTarget } from "../components/DeleteInstanceDialog";
import { uiStore, toast, promptText } from "../stores/ui";
import { t } from "../i18n";
import { initialOf, relativeTime, elidePath } from "../lib/format";
import { playtimeLabel } from "../lib/playtime";
import { loaderLabel } from "../lib/loaders";
import { readPref, writePref } from "../lib/prefs";
import { DeleteInstanceDialog } from "../components/DeleteInstanceDialog";
import {
  DirectoryManageDialog,
  DirectoryScanDialog,
} from "../components/GameDirectoryDialogs";

type SortKey = "lastPlayed" | "name" | "created";
type ViewMode = "cards" | "list";

export default function Instances(): JSX.Element {
  const navigate = useNavigate();
  const [search, setSearch] = createSignal("");
  const [sort, setSort] = createSignal<SortKey>(readPref("instances.sort", "lastPlayed"));
  const [view, setView] = createSignal<ViewMode>(readPref("instances.view", "cards"));
  const [manageOpen, setManageOpen] = createSignal(false);
  const [scanOpen, setScanOpen] = createSignal(false);
  const [detailTarget, setDetailTarget] = createSignal<DeleteInstanceTarget | null>(null);
  const [deleteOpen, setDeleteOpen] = createSignal(false);
  const [switching, setSwitching] = createSignal<string | null>(null);

  onMount(() => {
    void (async () => {
      await instancesStore.refreshDirectories(true);
      await instancesStore.rescan();
    })();
  });

  const directory = () => instancesStore.selectedDirectory();

  const items = createMemo(() => {
    const query = search().trim().toLowerCase();
    const list = (instancesStore.items() ?? []).filter((item) =>
      query
        ? item.summary.display_name.toLowerCase().includes(query) ||
          item.summary.id.toLowerCase().includes(query) ||
          item.summary.game_version.toLowerCase().includes(query)
        : true
    );
    const sorted = [...list];
    sorted.sort((a, b) => {
      switch (sort()) {
        case "name":
          return a.summary.display_name.localeCompare(b.summary.display_name);
        case "created":
          return (a.summary.created_at ?? "").localeCompare(b.summary.created_at ?? "");
        default:
          return (b.summary.last_played ?? "").localeCompare(a.summary.last_played ?? "");
      }
    });
    return sorted;
  });

  function setViewMode(next: ViewMode): void {
    setView(next);
    writePref("instances.view", next);
  }

  async function switchDirectory(id: string): Promise<void> {
    if (id === directory()?.directory.id) return;
    setSwitching(id);
    const res = await instancesStore.selectDirectory(id);
    setSwitching(null);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function addDirectory(): Promise<void> {
    const picked = await dialog.pickDirectory();
    if (!picked.ok || !picked.data) return;
    const folder = picked.data.split(/[\\/]/).filter(Boolean).pop() ?? "Minecraft";
    const nickname = await promptText({
      title: t("instances.sidebar.add"),
      label: t("instances.dir.renameHint"),
      initial: folder,
    });
    const res = await instancesStore.addDirectory(picked.data, nickname?.trim() || folder);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    await instancesStore.selectDirectory(res.data.id);
    toast({ message: t("instances.root.added", { path: picked.data }), tone: "success" });
  }

  async function removeDirectory(entry: GameDirectoryOverview): Promise<void> {
    const allowed = await uiStore.confirm({
      title: t("instances.root.remove.title"),
      body: t("instances.root.remove.body", { path: entry.directory.path }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!allowed) return;
    const res = await instancesStore.removeDirectory(entry.directory.id);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    toast({ message: t("instances.dir.removed"), tone: "success" });
  }

  async function launch(key: string, name: string): Promise<void> {
    const res = await launchStore.start(key, name);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  async function renameDialog(item: InstanceDetail): Promise<void> {
    const next = await promptText({
      title: t("instances.menu.rename"),
      label: t("instances.rename.prompt"),
      initial: item.summary.display_name,
    });
    if (!next || next === item.summary.display_name) return;
    const res = await instances.updateSettings(item.summary.key, { display_name: next });
    if (res.ok) {
      instancesStore.upsert(res.data);
      toast({ message: t("instances.renamed"), tone: "success" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function duplicateDialog(item: InstanceDetail): Promise<void> {
    const next = await promptText({
      title: t("instances.menu.duplicate"),
      label: t("instances.rename.prompt"),
      initial: `${item.summary.id}-copy`,
    });
    if (!next) return;
    const res = await instances.duplicate(item.summary.key, next, false);
    if (res.ok) {
      instancesStore.upsert(res.data);
      toast({ message: t("instances.duplicated"), tone: "success" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  function openDelete(item: InstanceDetail): void {
    setDetailTarget({
      key: item.summary.key,
      name: item.summary.display_name,
      external: item.directory.kind === "external",
    });
    setDeleteOpen(true);
  }

  const menuFor = (item: InstanceDetail) => {
    const key = item.summary.key;
    const running = launchStore.isRunning(key);
    return [
      {
        label: t("instances.menu.launch"),
        icon: "play" as const,
        disabled: running,
        onSelect: () => void launch(key, item.summary.display_name),
      },
      {
        label: t("instances.menu.detail"),
        icon: "settings" as const,
        onSelect: () => navigate(`/instances/${encodeURIComponent(key)}`),
      },
      {
        label: t("instances.menu.browseMods"),
        icon: "package" as const,
        onSelect: () => navigate(`/download?instance=${encodeURIComponent(key)}#mods`),
      },
      {
        label: t("instances.menu.rename"),
        icon: "edit" as const,
        disabled: running,
        onSelect: () => void renameDialog(item),
      },
      {
        label: t("instances.menu.duplicate"),
        icon: "copy" as const,
        disabled: running,
        onSelect: () => void duplicateDialog(item),
      },
      {
        label: t("instances.menu.delete"),
        icon: "trash" as const,
        danger: true,
        disabled: running,
        onSelect: () => openDelete(item),
      },
    ];
  };

  return (
    <div class="page">
      <PageHeader
        title={t("instances.title")}
        actions={
          <RmButton
            size="sm"
            variant="primary"
            icon="plus"
            onClick={() => navigate("/download#install")}
          >
            {t("instances.new")}
          </RmButton>
        }
      />
      <div class="page-body page-with-sidetabs instances-layout">
        <aside class="rm-sidetabs instances-dirs" aria-label={t("instances.sidebar.title")}>
          <div class="instances-dirs-head">
            <span>{t("instances.sidebar.title")}</span>
            <span class="instances-dirs-head-actions">
              <RmIconButton
                icon="plus"
                label={t("instances.sidebar.add")}
                onClick={() => void addDirectory()}
              />
              <RmIconButton
                icon="search"
                label={t("instances.sidebar.scan")}
                onClick={() => setScanOpen(true)}
              />
            </span>
          </div>
          <div class="instances-dirs-list">
            <For each={instancesStore.directories() ?? []}>
              {(entry) => (
                <button
                  class={`rm-sidetab instances-dir${entry.is_selected ? " is-active" : ""}${
                    switching() === entry.directory.id ? " is-busy" : ""
                  }`}
                  onClick={() => void switchDirectory(entry.directory.id)}
                  title={entry.directory.path}
                >
                  <Icon name="folder" size={16} />
                  <span class="instances-dir-main">
                    <span class="instances-dir-name">{entry.directory.name}</span>
                    <span class="instances-dir-path">
                      {elidePath(entry.directory.path, 24)}
                    </span>
                  </span>
                  <Show when={entry.directory.kind === "external"}>
                    <span class="instances-dir-external" title={t("instances.root.external")}>
                      <Icon name="external" size={12} />
                    </span>
                  </Show>
                  <span class="instances-dir-count">{entry.instance_count}</span>
                </button>
              )}
            </For>
            <Show when={instancesStore.directories() === undefined && !instancesStore.directoriesError()}>
              <div class="instances-dirs-skeleton" aria-hidden="true">
                <div /><div /><div />
              </div>
            </Show>
            <Show when={instancesStore.directoriesError()}>
              <RmErrorState
                message={instancesStore.directoriesError()!}
                onRetry={() => void instancesStore.refreshDirectories(true)}
              />
            </Show>
          </div>
          <div class="rm-sidetabs-footer">
            <RmButton size="sm" icon="sliders" onClick={() => setManageOpen(true)}>
              {t("instances.sidebar.manage")}
            </RmButton>
          </div>
        </aside>

        <div class="side-tab-content instances-content">
          <Show
            when={directory()}
            fallback={
              <Show
                when={instancesStore.directories() === undefined && instancesStore.directoriesLoading()}
                fallback={
                  <RmEmptyState
                    icon="folder"
                    title={t("instances.empty.directory.title")}
                    hint={t("instances.empty.directory.hint")}
                    action={
                      <RmButton variant="primary" icon="plus" onClick={() => void addDirectory()}>
                        {t("instances.sidebar.add")}
                      </RmButton>
                    }
                  />
                }
              >
                <div class="instance-grid">
                  <For each={[1, 2, 3]}>{() => <div class="instance-card is-skeleton" />}</For>
                </div>
              </Show>
            }
          >
            {(entry) => (
              <>
                <div class="instances-dir-header">
                  <div class="instances-dir-heading">
                    <div class="instances-dir-title">
                      <h2>{entry().directory.name}</h2>
                      <Show when={entry().directory.kind === "external"}>
                        <RmTag tone="success">{t("instances.root.external")}</RmTag>
                      </Show>
                      <Show when={entry().is_selected}>
                        <RmTag tone="accent">{t("instances.dir.default")}</RmTag>
                      </Show>
                      <Show when={entry().layout.has_hmcl_metadata}>
                        <RmTag title={t("instances.dir.hmclHint")}>HMCL</RmTag>
                      </Show>
                      <Show when={entry().layout.has_pcl_metadata}>
                        <RmTag title={t("instances.dir.pclHint")}>PCL</RmTag>
                      </Show>
                      <Show when={!entry().layout.has_versions}>
                        <RmTag tone="neutral">{t("instances.dir.notStandard")}</RmTag>
                      </Show>
                    </div>
                    <div class="instances-dir-path-line" title={entry().directory.path}>
                      {elidePath(entry().directory.path, 88)}
                    </div>
                  </div>
                  <div class="instances-dir-actions">
                    <RmButton
                      size="sm"
                      icon="folder"
                      onClick={() => void instancesStore.revealDirectory(entry().directory.id)}
                    >
                      {t("instances.dir.reveal")}
                    </RmButton>
                    <RmButton
                      size="sm"
                      icon="refresh"
                      onClick={() => void instancesStore.rescan()}
                    >
                      {t("common.refresh")}
                    </RmButton>
                    <RmButton
                      size="sm"
                      variant="danger"
                      icon="trash"
                      disabled={instancesStore.directories()!.length <= 1}
                      onClick={() => void removeDirectory(entry())}
                    >
                      {t("common.delete")}
                    </RmButton>
                  </div>
                </div>

                <Show when={!entry().layout.has_versions}>
                  <div class="instances-warning">
                    <Icon name="alert" size={16} />
                    <span>{t("instances.dir.notStandardHint")}</span>
                    <RmButton
                      size="sm"
                      icon="folder"
                      onClick={() => void instancesStore.revealDirectory(entry().directory.id)}
                    >
                      {t("instances.dir.reveal")}
                    </RmButton>
                  </div>
                </Show>

                <div class="toolbar instances-toolbar">
                  <RmInput
                    value={search()}
                    onInput={setSearch}
                    placeholder={t("instances.search.placeholder")}
                  />
                  <RmSelect
                    class="instances-sort-select"
                    value={sort()}
                    onChange={(value) => {
                      setSort(value as SortKey);
                      writePref("instances.sort", value);
                    }}
                    options={[
                      { value: "lastPlayed", label: t("instances.sort.lastPlayed") },
                      { value: "name", label: t("instances.sort.name") },
                      { value: "created", label: t("instances.sort.created") },
                    ]}
                  />
                  <div class="toolbar-divider" aria-hidden="true" />
                  <div class="view-toggle" role="group" aria-label={t("instances.view.label")}>
                    <RmIconButton
                      icon="grid"
                      label={t("instances.view.cards")}
                      active={view() === "cards"}
                      onClick={() => setViewMode("cards")}
                    />
                    <RmIconButton
                      icon="list"
                      label={t("instances.view.list")}
                      active={view() === "list"}
                      onClick={() => setViewMode("list")}
                    />
                  </div>
                </div>

                <RmContentState
                  data={instancesStore.items()}
                  loading={instancesStore.loading() && instancesStore.items() === undefined}
                  error={instancesStore.error()}
                  isEmpty={(list) => list.length === 0}
                  skeleton={
                    <Show
                      when={view() === "cards"}
                      fallback={
                        <div class="instance-list is-skeleton">
                          <For each={[1, 2, 3, 4]}>{() => <div class="instance-row is-skeleton" />}</For>
                        </div>
                      }
                    >
                      <div class="instance-grid">
                        <For each={[1, 2, 3]}>{() => <div class="instance-card is-skeleton" />}</For>
                      </div>
                    </Show>
                  }
                  empty={
                    <RmEmptyState
                      icon="gamepad"
                      title={t("instances.empty.title")}
                      hint={t("instances.empty.hint")}
                      action={
                        <div class="btn-row">
                          <RmButton
                            variant="primary"
                            icon="download"
                            onClick={() => navigate("/download#install")}
                          >
                            {t("instances.empty.cta")}
                          </RmButton>
                          <RmButton
                            icon="folder"
                            onClick={() => void instancesStore.revealDirectory(entry().directory.id)}
                          >
                            {t("instances.dir.reveal")}
                          </RmButton>
                        </div>
                      }
                    />
                  }
                >
                  {() => (
                    <Switch
                      fallback={
                        <InstanceCards
                          items={items()}
                          menuFor={menuFor}
                          onLaunch={launch}
                          onOpen={(key) => navigate(`/instances/${encodeURIComponent(key)}`)}
                        />
                      }
                    >
                      <Match when={view() === "list"}>
                        <InstanceList
                          items={items()}
                          menuFor={menuFor}
                          onLaunch={launch}
                          onOpen={(key) => navigate(`/instances/${encodeURIComponent(key)}`)}
                        />
                      </Match>
                    </Switch>
                  )}
                </RmContentState>
              </>
            )}
          </Show>
        </div>
      </div>

      <Show when={detailTarget()}>
        <DeleteInstanceDialog
          instance={detailTarget()!}
          open={deleteOpen()}
          onClose={() => setDeleteOpen(false)}
        />
      </Show>

      <DirectoryManageDialog
        open={manageOpen()}
        onClose={() => setManageOpen(false)}
        onAdd={() => void addDirectory()}
        onScan={() => {
          setManageOpen(false);
          setScanOpen(true);
        }}
      />
      <DirectoryScanDialog open={scanOpen()} onClose={() => setScanOpen(false)} />
    </div>
  );
}

// ── Card view ───────────────────────────────────────────────────────

function InstanceCards(props: {
  items: InstanceDetail[];
  menuFor: (item: InstanceDetail) => MenuItem[];
  onLaunch: (key: string, name: string) => void;
  onOpen: (key: string) => void;
}): JSX.Element {
  return (
    <div class="instance-grid">
      <For each={props.items}>
        {(item) => (
          <div
            class="instance-card"
            onClick={() => props.onOpen(item.summary.key)}
          >
            <Show when={launchStore.isRunning(item.summary.key)}>
              <span class="instance-running-dot" title={t("instances.running")} />
            </Show>
            <div class="instance-card-head">
              <InstanceIcon item={item} large />
              <div class="instance-card-titles">
                <div class="instance-card-name">{item.summary.display_name}</div>
                <div class="instance-card-id">{item.summary.id}</div>
              </div>
            </div>
            <div class="instance-card-tags">
              <RmTag>{item.summary.game_version}</RmTag>
              <Show when={item.summary.loader}>
                <RmTag tone="accent">{loaderLabel(item.summary.loader!.kind)}</RmTag>
              </Show>
              <Show when={item.directory.kind === "external"}>
                <RmTag tone="success">{t("instances.root.external")}</RmTag>
              </Show>
            </div>
            <div class="instance-card-meta">
              {item.summary.play_time_seconds > 0
                ? t("playtime.cumulative", {
                    time: playtimeLabel(Number(item.summary.play_time_seconds)),
                  })
                : t("playtime.none")}
              <Show when={item.summary.last_played}>
                {" "}· {relativeTime(item.summary.last_played!)}
              </Show>
            </div>
            <div class="instance-card-actions" onClick={(e) => e.stopPropagation()}>
              <LaunchControl item={item} onLaunch={props.onLaunch} />
              <RmMenu align="right" items={props.menuFor(item)}>
                <RmIconButton icon="more-horizontal" label={t("common.more")} />
              </RmMenu>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}

// ── List view ───────────────────────────────────────────────────────

function InstanceList(props: {
  items: InstanceDetail[];
  menuFor: (item: InstanceDetail) => MenuItem[];
  onLaunch: (key: string, name: string) => void;
  onOpen: (key: string) => void;
}): JSX.Element {
  return (
    <div class="instance-list">
      <div class="instance-row instance-row-head" aria-hidden="true">
        <span />
        <span class="instance-row-head-main">{t("instances.list.instance")}</span>
        <span class="instance-row-head-version">{t("instances.list.version")}</span>
        <span class="instance-row-head-meta">{t("instances.list.lastPlayed")}</span>
        <span />
      </div>
      <For each={props.items}>
        {(item) => (
          <div class="instance-row" onClick={() => props.onOpen(item.summary.key)}>
            <InstanceIcon item={item} />
            <div class="instance-row-main">
              <div class="instance-row-title-line">
                <span class="instance-row-name">{item.summary.display_name}</span>
                <Show when={launchStore.isRunning(item.summary.key)}>
                  <span class="instance-row-running">
                    <span class="dot-running" />
                    {t("instances.running")}
                  </span>
                </Show>
                <Show when={item.directory.kind === "external"}>
                  <RmTag tone="success">{t("instances.root.external")}</RmTag>
                </Show>
              </div>
              <div class="instance-row-sub">
                <span class="instance-row-id">{item.summary.id}</span>
                <Show when={item.summary.play_time_seconds > 0}>
                  <span class="instance-row-meta">
                    {t("playtime.cumulative", {
                      time: playtimeLabel(Number(item.summary.play_time_seconds)),
                    })}
                  </span>
                </Show>
              </div>
            </div>
            <div class="instance-row-version">
              <RmTag>{item.summary.game_version}</RmTag>
              <Show when={item.summary.loader}>
                <RmTag tone="accent">{loaderLabel(item.summary.loader!.kind)}</RmTag>
              </Show>
            </div>
            <div class="instance-row-lastPlayed">
              {item.summary.last_played ? relativeTime(item.summary.last_played) : t("instances.neverPlayed")}
            </div>
            <div class="instance-row-actions" onClick={(e) => e.stopPropagation()}>
              <LaunchControl item={item} onLaunch={props.onLaunch} compact />
              <RmMenu align="right" items={props.menuFor(item)}>
                <RmIconButton icon="more-horizontal" label={t("common.more")} />
              </RmMenu>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}

function LaunchControl(props: {
  item: InstanceDetail;
  onLaunch: (key: string, name: string) => void;
  compact?: boolean;
}): JSX.Element {
  return (
    <Show
      when={launchStore.isRunning(props.item.summary.key)}
      fallback={
        <RmButton
          size="sm"
          icon="play"
          title={t("instances.menu.launch")}
          onClick={() => props.onLaunch(props.item.summary.key, props.item.summary.display_name)}
        >
          <Show when={!props.compact}>{t("common.launch")}</Show>
        </RmButton>
      }
    >
      <RmButton
        size="sm"
        variant="danger"
        icon="square"
        onClick={() => void launchStore.stop(props.item.summary.key)}
      >
        <Show when={!props.compact}>{t("common.stop")}</Show>
      </RmButton>
    </Show>
  );
}

function InstanceIcon(props: { item: InstanceDetail; large?: boolean }): JSX.Element {
  const url = () => {
    const icon = props.item.summary.icon;
    if (typeof icon === "object" && icon !== null && "type" in icon) {
      if (icon.type === "data_url") return String((icon as { value: string }).value);
      if (icon.type === "file") return String((icon as { value: string }).value);
    }
    return null;
  };
  return (
    <div class={`instance-icon${props.large ? " is-large" : ""}`}>
      <Show when={url()} fallback={<span>{initialOf(props.item.summary.display_name)}</span>}>
        <img src={url()!} alt="" loading="lazy" />
      </Show>
    </div>
  );
}
