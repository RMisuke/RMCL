/**
 * Instance list (ui-design 03): card grid / compact list, search + sort,
 * inline launch/stop, context menu, single CTA for creation.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmTag } from "../components/rm/controls";
import { RmMenu } from "../components/rm/overlays";
import { RmContentState, RmEmptyState } from "../components/rm/surfaces";
import { profilesStore } from "../stores/profiles";
import { launchStore } from "../stores/launch";
import { profiles } from "../ipc/api";
import { uiStore, toast, confirmDanger, promptText } from "../stores/ui";
import { t } from "../i18n";
import { initialOf, relativeTime } from "../lib/format";
import { readPref, writePref } from "../lib/prefs";
import { Icon } from "../components/icons";

type SortKey = "lastPlayed" | "name" | "created";

export default function Instances(): JSX.Element {
  const navigate = useNavigate();
  const [search, setSearch] = createSignal("");
  const [sort, setSort] = createSignal<SortKey>(readPref("instances.sort", "lastPlayed"));
  const [view, setView] = createSignal<"cards" | "list">(readPref("instances.view", "cards"));

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
      </div>
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
