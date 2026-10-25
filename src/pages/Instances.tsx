/**
 * Instance list (ui-design 03): card grid / compact list, search + sort,
 * inline launch/stop, context menu, single CTA for creation.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmTag } from "../components/rm/controls";
import { RmMenu, RmDialog } from "../components/rm/overlays";
import { RmContentState, RmEmptyState, RmListRow } from "../components/rm/surfaces";
import { profilesStore } from "../stores/profiles";
import { launchStore } from "../stores/launch";
import { profiles, instances, dialog } from "../ipc/api";
import { settingsStore } from "../stores/settings";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { t } from "../i18n";
import { initialOf, relativeTime, elidePath } from "../lib/format";
import type { InstanceInfo } from "../ipc/types";
import { readPref, writePref } from "../lib/prefs";
import { Icon } from "../components/icons";

type SortKey = "lastPlayed" | "name" | "created";

export default function Instances(): JSX.Element {
  const navigate = useNavigate();
  const [search, setSearch] = createSignal("");
  const [sort, setSort] = createSignal<SortKey>(readPref("instances.sort", "lastPlayed"));
  const [view, setView] = createSignal<"cards" | "list">(readPref("instances.view", "cards"));
  // 多游戏目录："" = 全部根合并；路径值 = 只看该根下的版本。
  const [rootFilter, setRootFilter] = createSignal("");

  onMount(() => {
    void settingsStore.refresh();
  });

  /** 主根（配置的 game_root_dir）+ 额外根。 */
  const roots = createMemo(() => {
    const config = settingsStore.config();
    const primary = config?.game_root_dir ?? "";
    const extras = config?.extra_game_roots ?? [];
    return { primary, extras };
  });

  const items = createMemo(() => {
    const query = search().trim().toLowerCase();
    const list = (profilesStore.items() ?? []).filter((p) =>
      query ? p.name.toLowerCase().includes(query) : true
    );
    const sorted = [...list];
    sorted.sort((a, b) => {
      switch (sort()) {
        case "name":
          return a.name.localeCompare(b.name);
        case "created":
          return a.created_at.localeCompare(b.created_at);
        default:
          return (b.last_played ?? "").localeCompare(a.last_played ?? "");
      }
    });
    return sorted;
  });

  /** “浏览其他游戏目录”的版本清单（不并入 profile 列表，只读展示）。 */
  const [rootVersions, setRootVersions] = createSignal<InstanceInfo[] | null>(null);
  const [manageOpen, setManageOpen] = createSignal(false);
  const [rootLoading, setRootLoading] = createSignal(false);

  async function refreshRootVersions(root: string): Promise<void> {
    if (!root) {
      setRootVersions(null);
      return;
    }
    setRootLoading(true);
    if (root === "__all__") {
      // 全部根：主仓库（无参调用 = 后端已索引的根）+ 额外根合并去重。
      const results = await Promise.all([
        instances.list(),
        ...roots().extras.map((r) => instances.list(r)),
      ]);
      const merged = results.flatMap((res) => (res.ok ? res.data : []));
      const seen = new Set<string>();
      setRootVersions(merged.filter((v) => (seen.has(v.id) ? false : seen.add(v.id))));
    } else {
      // "__primary__" → 后端主仓库；否则按路径扫描额外根。
      const res = instances.list(root === "__primary__" ? undefined : root);
      const out = await res;
      setRootVersions(out.ok ? out.data : []);
      if (!out.ok) toast({ message: out.error.message, tone: "error" });
    }
    setRootLoading(false);
  }

  createEffect(() => {
    void refreshRootVersions(rootFilter());
  });

  async function addGameRoot(): Promise<void> {
    const res = await dialog.pickDirectory();
    if (!res.ok || !res.data) return;
    const picked = res.data;
    const { extras } = roots();
    if (extras.includes(picked)) {
      toast({ message: t("instances.root.alreadyAdded"), tone: "info" });
      setRootFilter(picked);
      return;
    }
    const saved = await settingsStore.update({ extra_game_roots: [...extras, picked] });
    toast({
      message: saved.ok
        ? t("instances.root.added", { path: picked })
        : (saved.message ?? t("instances.root.addFailed")),
      tone: saved.ok ? "success" : "error",
    });
  }

  async function removeGameRoot(root: string): Promise<void> {
    const allowed = await confirmDanger({
      title: t("instances.root.remove.title"),
      body: t("instances.root.remove.body", { path: root }),
      confirmLabel: t("common.delete"),
    });
    if (!allowed) return;
    const res = await settingsStore.update({
      extra_game_roots: roots().extras.filter((r) => r !== root),
    });
    if (res.ok && rootFilter() === root) setRootFilter("");
  }

  async function launch(profileId: string, name?: string): Promise<void> {
    const res = await launchStore.start(profileId, name);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  async function remove(id: string, name: string): Promise<void> {
    const allowed = await confirmDanger({
      title: t("instances.delete.title"),
      body: t("instances.delete.body", { name }),
      confirmLabel: t("instances.delete.confirm"),
      typeToConfirm: name,
    });
    if (!allowed) return;
    const res = await profiles.delete(id);
    if (res.ok) {
      profilesStore.removeItem(id);
      toast({ message: t("instances.deleted"), tone: "success" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  const menuFor = (id: string, name: string) => [
    { label: t("instances.menu.launch"), icon: "play" as const, disabled: launchStore.isRunning(id), onSelect: () => void launch(id) },
    { label: t("instances.menu.detail"), icon: "settings" as const, onSelect: () => navigate(`/instances/${id}`) },
    { label: t("instances.menu.browseMods"), icon: "package" as const, onSelect: () => navigate(`/download#mods?instance=${id}`) },
    { label: t("instances.menu.rename"), icon: "edit" as const, disabled: launchStore.isRunning(id), onSelect: () => void renameDialog(id, name) },
    { label: t("instances.menu.delete"), icon: "trash" as const, danger: true, disabled: launchStore.isRunning(id), onSelect: () => void remove(id, name) },
  ];

  async function renameDialog(id: string, current: string): Promise<void> {
    const next = await promptText({
      title: t("instances.menu.rename"),
      label: t("instances.rename.prompt"),
      initial: current,
    });
    if (!next || next === current) return;
    const res = await profiles.update(id, { name: next });
    if (res.ok) {
      profilesStore.upsert(res.data);
      toast({ message: t("instances.renamed"), tone: "success" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

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
        <div class="toolbar">
          <RmInput value={search()} onInput={setSearch} placeholder={t("instances.search.placeholder")} />
          {/* 游戏目录过滤（HMCL 式）：下拉只做过滤；目录的添加/移除走管理弹窗 */}
          <RmSelect
            value={rootFilter()}
            onChange={setRootFilter}
            options={[
              { value: "", label: t("instances.root.profiles") },
              { value: "__all__", label: t("instances.root.all") },
              { value: "__primary__", label: t("instances.root.primary") },
              ...roots().extras.map((r) => ({ value: r, label: elidePath(r, 36) })),
            ]}
          />
          <RmIconButton
            icon="folder"
            label={t("instances.root.manage")}
            title={t("instances.root.manage")}
            onClick={() => setManageOpen(true)}
          />
          <RmSelect
            value={sort()}
            onChange={(v) => {
              setSort(v as SortKey);
              writePref("instances.sort", v);
            }}
            options={[
              { value: "lastPlayed", label: t("instances.sort.lastPlayed") },
              { value: "name", label: t("instances.sort.name") },
              { value: "created", label: t("instances.sort.created") },
            ]}
          />
          <div class="view-toggle">
            <RmIconButton
              icon="grid"
              label={t("instances.view.cards")}
              onClick={() => {
                setView("cards");
                writePref("instances.view", "cards");
              }}
            />
            <RmIconButton
              icon="list"
              label={t("instances.view.list")}
              onClick={() => {
                setView("list");
                writePref("instances.view", "list");
              }}
            />
          </div>
          <RmIconButton icon="refresh" label={t("common.refresh")} onClick={() => void profilesStore.refresh(true)} />
        </div>

        <Show
          when={rootFilter() === ""}
          fallback={
            <div class="rm-card">
              <div class="rm-card-head">
                <h3 class="rm-card-title">
                  {rootFilter() === "__all__"
                    ? t("instances.root.allTitle")
                    : rootFilter() === "__primary__"
                      ? t("instances.root.primary")
                      : t("instances.root.browseTitle", { path: elidePath(rootFilter(), 44) })}
                </h3>
                <span class="muted-line">
                  {rootLoading() ? t("common.loading") : `${rootVersions()?.length ?? 0}`}
                </span>
              </div>
              <div class="rm-card-body">
                <div class="rm-list">
                  <For each={rootVersions() ?? []}>
                    {(version) => (
                      <RmListRow
                        icon={<InstanceIcon profile={{ name: version.id, icon: null }} />}
                        title={version.id}
                        subtitle={`${version.game_version}${version.modded ? " · modded" : ""}`}
                        trailing={
                          <RmTag tone={version.missing_count === 0 ? "success" : "warning"}>
                            {version.missing_count === 0
                              ? t("settings.advanced.repo.complete")
                              : t("settings.advanced.repo.missing", { n: version.missing_count })}
                          </RmTag>
                        }
                      />
                    )}
                  </For>
                  <Show when={!rootLoading() && (rootVersions()?.length ?? 0) === 0}>
                    <div class="hero-empty">{t("instances.root.noVersions")}</div>
                  </Show>
                </div>
              </div>
            </div>
          }
        >
        <RmContentState
          data={profilesStore.items()}
          loading={profilesStore.loading() && profilesStore.items() === undefined}
          error={profilesStore.error()}
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
          {(list) => (
            <Show
              when={view() === "cards"}
              fallback={
                <div class="rm-list">
                  <For each={items()}>
                    {(profile) => (
                      <div class="rm-row is-clickable" onClick={() => navigate(`/instances/${profile.id}`)}>
                        <InstanceIcon profile={profile} />
                        <div class="rm-row-main">
                          <div class="rm-row-title">{profile.name}</div>
                          <div class="rm-row-subtitle">
                            {profile.game_version}
                            <Show when={profile.mod_loader}> · {profile.mod_loader!.loader_type} {profile.mod_loader!.loader_version}</Show>
                          </div>
                        </div>
                        <div class="rm-row-trailing">
                          <Show when={launchStore.isRunning(profile.id)} fallback={
                            <RmButton size="sm" icon="play" onClick={(e) => { e.stopPropagation(); void launch(profile.id, profile.name); }}>
                              {t("common.launch")}
                            </RmButton>
                          }>
                            <RmButton size="sm" variant="danger" icon="square" onClick={(e) => { e.stopPropagation(); void launchStore.kill(profile.id); }}>
                              {t("common.stop")}
                            </RmButton>
                          </Show>
                          <RmMenu align="right" items={menuFor(profile.id, profile.name)}>
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
                  {(profile) => (
                    <div class="instance-card" onClick={() => navigate(`/instances/${profile.id}`)}>
                      <Show when={launchStore.isRunning(profile.id)}>
                        <span class="instance-running-dot" title={t("instances.running")} />
                      </Show>
                      <InstanceIcon profile={profile} large />
                      <div class="instance-card-name">{profile.name}</div>
                      <div class="instance-card-tags">
                        <RmTag>{profile.game_version}</RmTag>
                        <Show when={profile.mod_loader}>
                          <RmTag tone="accent">{profile.mod_loader!.loader_type}</RmTag>
                        </Show>
                      </div>
                      <div class="instance-card-meta">
                        {profile.last_played ? relativeTime(profile.last_played) : t("instances.neverPlayed")}
                      </div>
                      <div class="instance-card-actions" onClick={(e) => e.stopPropagation()}>
                        <Show
                          when={launchStore.isRunning(profile.id)}
                          fallback={
                            <RmButton size="sm" icon="play" onClick={() => void launch(profile.id, profile.name)}>
                              {t("common.launch")}
                            </RmButton>
                          }
                        >
                          <RmButton size="sm" variant="danger" icon="square" onClick={() => void launchStore.kill(profile.id)}>
                            {t("common.stop")}
                          </RmButton>
                        </Show>
                        <RmMenu align="right" items={menuFor(profile.id, profile.name)}>
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
        </Show>
      </div>

      {/* 游戏目录管理（HMCL 式弹窗）：主目录固定，额外目录可增删 */}
      <RmDialog
        open={manageOpen()}
        title={t("instances.root.manage")}
        onClose={() => setManageOpen(false)}
      >
        <div class="rm-list">
          <RmListRow
            icon={<InstanceIcon profile={{ name: "M", icon: null }} />}
            title={roots().primary ? elidePath(roots().primary, 44) : t("instances.root.primaryDefault")}
            subtitle={t("instances.root.primaryHint")}
            selected={rootFilter() === "__primary__"}
            onClick={() => {
              setRootFilter("__primary__");
              setManageOpen(false);
            }}
            trailing={<RmTag tone="accent">{t("instances.root.primary")}</RmTag>}
          />
          <For each={roots().extras}>
            {(root) => (
              <RmListRow
                icon={<InstanceIcon profile={{ name: "E", icon: null }} />}
                title={elidePath(root, 44)}
                subtitle={t("instances.root.extraHint")}
                selected={rootFilter() === root}
                onClick={() => {
                  setRootFilter(root);
                  setManageOpen(false);
                }}
                trailing={
                  <div class="btn-row">
                    <RmIconButton
                      icon="trash"
                      danger
                      label={t("instances.root.remove.title")}
                      onClick={() => void removeGameRoot(root)}
                    />
                  </div>
                }
              />
            )}
          </For>
        </div>
        <div class="btn-row" style={{ "margin-top": "12px" }}>
          <RmButton size="sm" icon="plus" onClick={() => void addGameRoot()}>
            {t("instances.root.add")}
          </RmButton>
        </div>
      </RmDialog>
    </div>
  );
}

function InstanceIcon(props: { profile: { name: string; icon: string | null }; large?: boolean }): JSX.Element {
  return (
    <div class={`instance-icon${props.large ? " is-large" : ""}`}>
      <Show when={props.profile.icon} fallback={<span>{initialOf(props.profile.name)}</span>}>
        <img src={props.profile.icon!} alt="" loading="lazy" />
      </Show>
    </div>
  );
}
