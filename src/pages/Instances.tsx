/**
 * Instance list (ADR-016): version-backed instances grouped by game directory.
 *
 * There is no separate "version repository" page anymore — an instance *is*
 * `versions/{id}`. The list deliberately skips the expensive completeness scan;
 * health checks live in the instance detail maintenance tab.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmTag } from "../components/rm/controls";
import { RmMenu, RmDialog } from "../components/rm/overlays";
import { RmContentState, RmEmptyState } from "../components/rm/surfaces";
import { instancesStore } from "../stores/instances";
import { launchStore } from "../stores/launch";
import { instances, gameDirectories, dialog } from "../ipc/api";
import type { GameDirectory, InstanceDetail } from "../ipc/types";
import type { DeleteInstanceTarget } from "../components/DeleteInstanceDialog";
import { uiStore, toast, promptText } from "../stores/ui";
import { t } from "../i18n";
import { initialOf, relativeTime, elidePath } from "../lib/format";
import { playtimeLabel } from "../lib/playtime";
import { loaderLabel } from "../lib/loaders";
import { readPref, writePref } from "../lib/prefs";
import { DeleteInstanceDialog } from "../components/DeleteInstanceDialog";

type SortKey = "lastPlayed" | "name" | "created";

export default function Instances(): JSX.Element {
  const navigate = useNavigate();
  const [search, setSearch] = createSignal("");
  const [sort, setSort] = createSignal<SortKey>(readPref("instances.sort", "lastPlayed"));
  const [view, setView] = createSignal<"cards" | "list">(readPref("instances.view", "cards"));
  const [directories, setDirectories] = createSignal<GameDirectory[]>([]);
  const [selectedDir, setSelectedDir] = createSignal("");
  const [detailTarget, setDetailTarget] = createSignal<DeleteInstanceTarget | null>(null);
  const [deleteOpen, setDeleteOpen] = createSignal(false);

  async function loadDirectories(): Promise<void> {
    const [all, selected] = await Promise.all([
      gameDirectories.list(),
      gameDirectories.selected(),
    ]);
    if (all.ok) setDirectories(all.data);
    if (selected.ok) setSelectedDir(selected.data.id);
    else if (all.ok && all.data[0]) setSelectedDir(all.data[0].id);
  }

  onMount(() => {
    void (async () => {
      await loadDirectories();
      await instancesStore.refresh(true);
    })();
  });

  const items = createMemo(() => {
    const query = search().trim().toLowerCase();
    const list = (instancesStore.items() ?? []).filter((item) =>
      query
        ? item.summary.display_name.toLowerCase().includes(query) ||
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

  const currentDirectory = createMemo(
    () => directories().find((d) => d.id === selectedDir()) ?? null
  );

  async function switchDirectory(id: string): Promise<void> {
    const res = await gameDirectories.select(id);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    setSelectedDir(id);
    await instancesStore.refresh(true);
  }

  async function addDirectory(): Promise<void> {
    const picked = await dialog.pickDirectory();
    if (!picked.ok || !picked.data) return;
    const res = await gameDirectories.add(picked.data, undefined, true);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    await loadDirectories();
    await switchDirectory(res.data.id);
    toast({ message: t("instances.root.added", { path: picked.data }), tone: "success" });
  }

  async function removeDirectory(directory: GameDirectory): Promise<void> {
    const allowed = await uiStore.confirm({
      title: t("instances.root.remove.title"),
      body: t("instances.root.remove.body", { path: directory.path }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!allowed) return;
    const res = await gameDirectories.remove(directory.id);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    await loadDirectories();
    await instancesStore.refresh(true);
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
          <RmButton size="sm" variant="primary" icon="plus" onClick={() => navigate("/download#install")}>
            {t("instances.new")}
          </RmButton>
        }
      />
      <div class="page-body">
        <div class="toolbar instances-toolbar">
          <RmInput value={search()} onInput={setSearch} placeholder={t("instances.search.placeholder")} />
          <RmSelect
            class="instances-root-select"
            value={selectedDir()}
            onChange={(value) => void switchDirectory(value)}
            options={directories().map((dir) => ({
              value: dir.id,
              label: dir.name || elidePath(dir.path, 24),
            }))}
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
          <div class="view-toggle">
            <RmIconButton
              icon="grid"
              label={t("instances.view.cards")}
              active={view() === "cards"}
              onClick={() => {
                setView("cards");
                writePref("instances.view", "cards");
              }}
            />
            <RmIconButton
              icon="list"
              label={t("instances.view.list")}
              active={view() === "list"}
              onClick={() => {
                setView("list");
                writePref("instances.view", "list");
              }}
            />
          </div>
          <RmIconButton
            icon="refresh"
            label={t("common.refresh")}
            onClick={() => void instancesStore.refresh(true)}
          />
          <RmIconButton icon="folder" label={t("instances.root.add")} onClick={() => void addDirectory()} />
          <Show when={currentDirectory() && directories().length > 1}>
            <RmIconButton
              icon="trash"
              danger
              label={t("instances.root.remove.title")}
              onClick={() => void removeDirectory(currentDirectory()!)}
            />
          </Show>
        </div>

        <Show when={currentDirectory()}>
          <div class="muted-line instance-directory-hint">
            {elidePath(currentDirectory()!.path, 64)}
          </div>
        </Show>

        <RmContentState
          data={instancesStore.items()}
          loading={instancesStore.loading() && instancesStore.items() === undefined}
          error={instancesStore.error()}
          isEmpty={(list) => list.length === 0}
          skeleton={
            <div class="instance-grid">
              <For each={[1, 2, 3]}>{() => <div class="instance-card is-skeleton" />}</For>
            </div>
          }
          empty={
            <RmEmptyState
              icon="gamepad"
              title={t("instances.empty.title")}
              hint={t("instances.empty.hint")}
              action={
                <RmButton variant="primary" icon="download" onClick={() => navigate("/download#install")}>
                  {t("instances.empty.cta")}
                </RmButton>
              }
            />
          }
        >
          {() => (
            <Show
              when={view() === "cards"}
              fallback={
                <div class="rm-list">
                  <For each={items()}>
                    {(item) => (
                      <div
                        class="rm-row is-clickable"
                        onClick={() => navigate(`/instances/${encodeURIComponent(item.summary.key)}`)}
                      >
                        <InstanceIcon item={item} />
                        <div class="rm-row-main">
                          <div class="rm-row-title">{item.summary.display_name}</div>
                          <div class="rm-row-subtitle">
                            {item.summary.game_version}
                            <Show when={item.summary.loader}>
                              {" "}· {loaderLabel(item.summary.loader!.kind)} {item.summary.loader!.version}
                            </Show>
                          </div>
                        </div>
                        <div class="rm-row-trailing" onClick={(e) => e.stopPropagation()}>
                          <Show
                            when={launchStore.isRunning(item.summary.key)}
                            fallback={
                              <RmButton
                                size="sm"
                                icon="play"
                                onClick={() =>
                                  void launch(item.summary.key, item.summary.display_name)
                                }
                              >
                                {t("common.launch")}
                              </RmButton>
                            }
                          >
                            <RmButton
                              size="sm"
                              variant="danger"
                              icon="square"
                              onClick={() => void launchStore.stop(item.summary.key)}
                            >
                              {t("common.stop")}
                            </RmButton>
                          </Show>
                          <RmMenu align="right" items={menuFor(item)}>
                            <RmIconButton icon="more-horizontal" label={t("common.more")} />
                          </RmMenu>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              }
            >
              <div class="instance-grid">
                <For each={items()}>
                  {(item) => (
                    <div
                      class="instance-card"
                      onClick={() => navigate(`/instances/${encodeURIComponent(item.summary.key)}`)}
                    >
                      <Show when={launchStore.isRunning(item.summary.key)}>
                        <span class="instance-running-dot" title={t("instances.running")} />
                      </Show>
                      <InstanceIcon item={item} large />
                      <div class="instance-card-name">{item.summary.display_name}</div>
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
                        <Show
                          when={launchStore.isRunning(item.summary.key)}
                          fallback={
                            <RmButton
                              size="sm"
                              icon="play"
                              onClick={() => void launch(item.summary.key, item.summary.display_name)}
                            >
                              {t("common.launch")}
                            </RmButton>
                          }
                        >
                          <RmButton
                            size="sm"
                            variant="danger"
                            icon="square"
                            onClick={() => void launchStore.stop(item.summary.key)}
                          >
                            {t("common.stop")}
                          </RmButton>
                        </Show>
                        <RmMenu align="right" items={menuFor(item)}>
                          <RmIconButton icon="more-horizontal" label={t("common.more")} />
                        </RmMenu>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          )}
        </RmContentState>
      </div>

      <Show when={detailTarget()}>
        <DeleteInstanceDialog
          instance={detailTarget()!}
          open={deleteOpen()}
          onClose={() => setDeleteOpen(false)}
        />
      </Show>
    </div>
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
