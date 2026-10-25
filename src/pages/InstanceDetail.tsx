/**
 * Instance detail (ui-design 04): five hash-tabs (overview / mods / loaders /
 * settings / maintenance), side toolbar with launch + browse/manage menus,
 * field-level inherit locks, dependency scan + one-click apply (pure logic).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, onCleanup, Switch, Match } from "solid-js";
import { useParams, useNavigate, useLocation } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { DeleteInstanceDialog } from "../components/DeleteInstanceDialog";
import { Icon } from "../components/icons";
import { RmButton, RmIconButton, RmInput, RmSelect, RmSwitch, RmTag, RmCheckbox } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import {
  RmCard,
  RmListRow,
  RmSideTabs,
  RmAdvancedSection,
  RmFieldRow,
  RmInheritLock,
  RmEmptyState,
  RmErrorState,
} from "../components/rm/surfaces";
import { profiles, instances, mods, modLoader, settings as settingsApi } from "../ipc/api";
import type {
  Profile,
  InstancePaths,
  ResolvedInstanceSettings,
  MissingFileInfo,
  JavaInfo,
  LoaderVersion,
  ModsDependencyScanView,
  AppliedPlanReport,
} from "../ipc/types";
import type { ProfileUpdateOpts } from "../ipc/api";
import { profilesStore } from "../stores/profiles";
import { launchStore } from "../stores/launch";
import { modloadersStore } from "../stores/modloaders";
import { downloadsStore } from "../stores/downloads";
import { uiStore, toast, promptText } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { relativeTime, elidePath } from "../lib/format";
import { LOADER_CARDS } from "../lib/loaders";
import { loaderIcon } from "../lib/mcIcons";
import * as versionCache from "../stores/versionCache";

const TABS: { id: string; icon: "list" | "package" | "layers" | "sliders" | "wrench"; label: TranslationKey }[] = [
  { id: "overview", icon: "list", label: "instanceDetail.tab.overview" },
  { id: "mods", icon: "package" as const, label: "instanceDetail.tab.mods" },
  { id: "loaders", icon: "layers" as const, label: "instanceDetail.tab.loaders" },
  { id: "settings", icon: "sliders" as const, label: "instanceDetail.tab.settings" },
  { id: "maintenance", icon: "wrench" as const, label: "instanceDetail.tab.maintenance" },
];

export default function InstanceDetail(): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const [tab, setTab] = createSignal(location.hash.replace("#", "") || "overview");

  const profileId = () => params.id ?? "";
  const profile = createMemo(
    () => profilesStore.items()?.find((p) => p.id === profileId()) ?? null
  );

  createEffect(() => {
    // Keep the hash in sync so refresh/back returns to the same tab.
    history.replaceState(null, "", `#${tab()}`);
  });

  async function launch(): Promise<void> {
    const res = await launchStore.start(profileId(), profile()?.name);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  return (
    <div class="page">
      <PageHeader
        crumbs={[{ label: t("instances.title"), href: "/instances" }]}
        title={profile()?.name ?? profileId()}
        actions={
          <div class="btn-row">
            <Show
              when={launchStore.isRunning(profileId())}
              fallback={
                <RmButton size="sm" variant="primary" icon="play" onClick={() => void launch()}>
                  {t("common.launch")}
                </RmButton>
              }
            >
              <RmButton size="sm" variant="danger" icon="square" onClick={() => void launchStore.kill(profileId())}>
                {t("common.stop")}
              </RmButton>
              <RmButton size="sm" icon="terminal" onClick={() => uiStore.openLogDrawer(params.id)}>
                {t("home.viewLog")}
              </RmButton>
            </Show>
          </div>
        }
      />
      <div class="page-body page-with-sidetabs">
        <Show
          when={profile()}
          fallback={
            <RmEmptyState
              icon="alert"
              title={t("instanceDetail.missing")}
              action={<RmButton onClick={() => navigate("/instances")}>{t("common.back")}</RmButton>}
            />
          }
        >
          {(p) => (
            <>
              <RmSideTabs
                items={TABS.map((tab_) => ({ ...tab_, label: t(tab_.label) }))}
                active={tab()}
                onSelect={setTab}
                footer={
                  <div class="instance-toolbar">
                    <RmButton size="sm" icon={launchStore.isRunning(p().id) ? "square" : "play"} variant={launchStore.isRunning(p().id) ? "danger" : "primary"} onClick={() => (launchStore.isRunning(p().id) ? void launchStore.kill(p().id) : void launch())}>
                      {launchStore.isRunning(p().id) ? t("common.stop") : t("common.launch")}
                    </RmButton>
                    <BrowseMenu profile={p()} />
                    <ManageMenu profile={p()} />
                  </div>
                }
              />
              <div class="side-tab-content">
                <Switch>
                  <Match when={tab() === "overview"}><OverviewTab profile={p()} /></Match>
                  <Match when={tab() === "mods"}><ModsTab profile={p()} /></Match>
                  <Match when={tab() === "loaders"}><LoadersTab profile={p()} /></Match>
                  <Match when={tab() === "settings"}><SettingsTab profile={p()} /></Match>
                  <Match when={tab() === "maintenance"}><MaintenanceTab profile={p()} /></Match>
                </Switch>
              </div>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}

// ── Toolbar menus ──

function BrowseMenu(props: { profile: Profile }): JSX.Element {
  const [paths, setPaths] = createSignal<InstancePaths | null>(null);
  createEffect(() => {
    void instances.paths(props.profile.id).then((res) => {
      if (res.ok) setPaths(res.data);
    });
  });
  const copy = (value: string) => {
    void navigator.clipboard.writeText(value);
    toast({ message: t("common.copied"), tone: "success" });
  };
  return (
    <RmMenu
      side="right"
      items={[
        { label: `${t("instanceDetail.browse.gameDir")}: ${elidePath(paths()?.game_root ?? "")}`, onSelect: () => paths() && copy(paths()!.game_root) },
        { label: `${t("instanceDetail.browse.runDir")}: ${elidePath(paths()?.run_dir ?? "")}`, onSelect: () => paths() && copy(paths()!.run_dir) },
        { label: `${t("instanceDetail.browse.mods")}: ${elidePath(paths()?.mods_dir ?? "")}`, onSelect: () => paths() && copy(paths()!.mods_dir) },
      ]}
    >
      <RmButton size="sm" icon="folder">{t("instanceDetail.browse")}</RmButton>
    </RmMenu>
  );
}

function ManageMenu(props: { profile: Profile }): JSX.Element {
  const navigate = useNavigate();
  const [deleteOpen, setDeleteOpen] = createSignal(false);
  return (
    <>
      <RmMenu
        // 侧栏底部按钮：向上弹出（红框位置），避免向下遮住按钮本身。
        side="up"
        items={[
          { label: t("instances.menu.rename"), icon: "edit", onSelect: () => void renameProfile(props.profile) },
          { label: t("instances.menu.browseMods"), icon: "package", onSelect: () => navigate(`/download#mods?instance=${props.profile.id}`) },
          { label: t("instanceDetail.manage.delete"), icon: "trash", danger: true, onSelect: () => setDeleteOpen(true) },
        ]}
      >
        <RmButton size="sm" icon="more-horizontal">{t("instanceDetail.manage")}</RmButton>
      </RmMenu>
      <DeleteInstanceDialog profile={props.profile} open={deleteOpen()} onClose={() => setDeleteOpen(false)} />
    </>
  );
}

async function renameProfile(profile: Profile): Promise<void> {
  // WKWebView 不支持 window.prompt —— 统一走 prompt 对话框。
  const next = await promptText({
    title: t("instances.menu.rename"),
    label: t("instances.rename.prompt"),
    initial: profile.name,
  });
  if (!next || next === profile.name) return;
  const res = await profiles.update(profile.id, { name: next });
  if (res.ok) {
    profilesStore.upsert(res.data);
    toast({ message: t("instances.renamed"), tone: "success" });
  } else {
    toast({ message: res.error.message, tone: "error" });
  }
}


// ── Tab 1 · Overview ──

function OverviewTab(props: { profile: Profile }): JSX.Element {
  const navigate = useNavigate();
  const [paths, setPaths] = createSignal<InstancePaths | null>(null);
  const [resolved, setResolved] = createSignal<ResolvedInstanceSettings | null>(null);

  createEffect(() => {
    void instances.paths(props.profile.id).then((r) => r.ok && setPaths(r.data));
    void instances.resolvedSettings(props.profile.id).then((r) => r.ok && setResolved(r.data));
  });

  const copy = (value: string) => {
    void navigator.clipboard.writeText(value);
    toast({ message: t("common.copied"), tone: "success" });
  };

  return (
    <div class="tab-stack">
      <RmCard title={t("instanceDetail.overview.identity")}>
        <div class="chip-row">
          <RmTag tone="accent">{props.profile.game_version}</RmTag>
          <Show when={props.profile.mod_loader}>
            <RmTag tone="accent">{props.profile.mod_loader!.loader_type} {props.profile.mod_loader!.loader_version}</RmTag>
          </Show>
          <Show when={resolved()}>
            <RmTag>{resolved()!.max_memory_mb} MiB</RmTag>
          </Show>
        </div>
        <div class="muted-line">
          {t("instanceDetail.overview.lastPlayed", {
            time: props.profile.last_played ? relativeTime(props.profile.last_played) : t("instances.neverPlayed"),
          })}
        </div>
      </RmCard>

      <RmCard title={t("instanceDetail.overview.paths")}>
        <For
          each={[
            { label: t("instanceDetail.browse.gameDir"), value: paths()?.game_root },
            { label: t("instanceDetail.browse.runDir"), value: paths()?.run_dir },
            { label: t("instanceDetail.browse.mods"), value: paths()?.mods_dir },
          ]}
        >
          {(row) => (
            <Show when={row.value}>
              <div class="path-row">
                <span class="path-row-label">{row.label}</span>
                <span class="path-row-value" title={row.value}>{elidePath(row.value!)}</span>
                <RmIconButton icon="copy" label={t("common.copy")} onClick={() => copy(row.value!)} />
              </div>
            </Show>
          )}
        </For>
      </RmCard>

      <RmCard title={t("instanceDetail.overview.quickActions")}>
        <div class="btn-row">
          <RmButton icon="package" onClick={() => navigate(`/download#mods?instance=${props.profile.id}`)}>
            {t("instanceDetail.overview.goMods")}
          </RmButton>
          <RmButton icon="shield" onClick={() => setTab("mods")}>
            {t("instanceDetail.mods.missingScan")}
          </RmButton>
          <RmButton icon="sliders" onClick={() => setTab("settings")}>
            {t("instanceDetail.tab.settings")}
          </RmButton>
        </div>
      </RmCard>
    </div>
  );
}

function setTab(id: string): void {
  history.replaceState(null, "", `#${id}`);
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

// ── Tab 2 · Mods management ──

function ModsTab(props: { profile: Profile }): JSX.Element {
  const navigate = useNavigate();
  const [enabled, setEnabled] = createSignal<string[]>([]);
  const [disabled, setDisabled] = createSignal<string[]>([]);
  const [search, setSearch] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [scan, setScan] = createSignal<ModsDependencyScanView | null>(null);
  const [scanOpen, setScanOpen] = createSignal(false);
  const [applying, setApplying] = createSignal(false);

  async function refreshMods(): Promise<void> {
    setLoading(true);
    const res = await mods.list(props.profile.id);
    setLoading(false);
    if (res.ok) {
      setEnabled(res.data.enabled);
      setDisabled(res.data.disabled);
      setError(null);
    } else {
      setError(res.error.message);
    }
  }

  onMount(() => void refreshMods());

  const filteredEnabled = createMemo(() =>
    enabled().filter((f) => f.toLowerCase().includes(search().toLowerCase()))
  );

  async function toggle(file: string, nextEnabled: boolean): Promise<void> {
    // Optimistic update; roll back on failure (04 §4).
    if (nextEnabled) {
      setDisabled((l) => l.filter((f) => f !== file));
      setEnabled((l) => [...l, file]);
    } else {
      setEnabled((l) => l.filter((f) => f !== file));
      setDisabled((l) => [...l, file]);
    }
    const res = await mods.toggle(props.profile.id, file, nextEnabled);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      void refreshMods();
    }
  }

  async function removeMod(file: string): Promise<void> {
    const allowed = await uiStore.confirm({
      title: t("instanceDetail.mods.remove.title"),
      body: t("instanceDetail.mods.remove.body", { file }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!allowed) return;
    const res = await mods.remove(props.profile.id, file);
    if (res.ok) {
      toast({ message: t("instanceDetail.mods.removed"), tone: "success" });
      void refreshMods();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function addLocal(): Promise<void> {
    const res = await mods.pickAndInstall(props.profile.id);
    if (res.ok && res.data) {
      toast({ message: t("instanceDetail.mods.added", { file: res.data }), tone: "success" });
      void refreshMods();
    } else if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function runScan(): Promise<void> {
    const res = await mods.dependencyScan(props.profile.id);
    if (res.ok) {
      setScan(res.data);
      setScanOpen(true);
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function applyPlan(): Promise<void> {
    setApplying(true);
    toast({ message: t("tasks.download.started", { name: t("instanceDetail.mods.missingScan") }), tone: "info" });
    const taskKey = downloadsStore.beginTask(
      t("tasks.applyPlan.label", { instance: props.profile.name })
    );
    const res: { ok: boolean; data?: AppliedPlanReport; message?: string } = await mods
      .applyPlan(props.profile.id)
      .then((r) => (r.ok ? { ok: true, data: r.data } : { ok: false, message: r.error.message }));
    setApplying(false);
    setScanOpen(false);
    if (res.ok && res.data) {
      downloadsStore.finishTask(taskKey, true, `${res.data.installed.length}`);
      toast({
        message: t("instanceDetail.mods.applied", { n: res.data.installed.length }),
        tone: "success",
      });
      void refreshMods();
    } else {
      downloadsStore.finishTask(taskKey, false, res.message);
      toast({ message: res.message ?? "", tone: "error" });
    }
  }

  return (
    <div class="tab-stack">
      <div class="toolbar">
        <RmButton size="sm" icon="refresh" onClick={() => void refreshMods()}>{t("common.refresh")}</RmButton>
        <RmButton size="sm" icon="plus" onClick={() => void addLocal()}>{t("instanceDetail.mods.addLocal")}</RmButton>
        <RmButton size="sm" icon="package" onClick={() => navigate(`/download#mods?instance=${props.profile.id}`)}>
          {t("instanceDetail.mods.goStore")}
        </RmButton>
        <RmButton size="sm" icon="shield" onClick={() => void runScan()}>{t("instanceDetail.mods.missingScan")}</RmButton>
        <RmInput value={search()} onInput={setSearch} placeholder={t("common.search")} />
      </div>

      <Show when={error()} fallback={<Show when={loading()}><div class="muted-line">{t("common.loading")}</div></Show>}>
        <RmErrorState message={error()!} onRetry={() => void refreshMods()} />
      </Show>

      <div class="rm-list">
        <For each={filteredEnabled()}>
          {(file) => (
            <RmListRow
              icon={<Icon name="package" size={18} />}
              title={<span class="mod-file">{file}</span>}
              subtitle={<span>mods/</span>}
              trailing={
                <div class="btn-row">
                  <RmCheckbox checked onChange={(next) => void toggle(file, next)} />
                  <RmMenu
                    align="right"
                    items={[
                      { label: t("instanceDetail.mods.menu.disable"), onSelect: () => void toggle(file, false) },
                      { label: t("common.delete"), icon: "trash", danger: true, onSelect: () => void removeMod(file) },
                      { label: t("common.copy"), onSelect: () => { void navigator.clipboard.writeText(file); toast({ message: t("common.copied"), tone: "success" }); } },
                    ]}
                  >
                    <RmIconButton icon="more-horizontal" label={t("common.more")} />
                  </RmMenu>
                </div>
              }
            />
          )}
        </For>
        <Show when={disabled().length > 0}>
          <div class="rm-list-divider">{t("instanceDetail.mods.disabledSection")}</div>
          <For each={disabled()}>
            {(file) => (
              <RmListRow
                icon={<Icon name="package" size={18} />}
                title={<span class="mod-file is-disabled">{file}</span>}
                subtitle={<span>mods/disabled/</span>}
                trailing={
                  <div class="btn-row">
                    <RmCheckbox checked={false} onChange={(next) => void toggle(file, next)} />
                    <RmMenu
                      align="right"
                      items={[
                        { label: t("instanceDetail.mods.menu.enable"), onSelect: () => void toggle(file, true) },
                        { label: t("common.delete"), icon: "trash", danger: true, onSelect: () => void removeMod(file) },
                      ]}
                    >
                      <RmIconButton icon="more-horizontal" label={t("common.more")} />
                    </RmMenu>
                  </div>
                }
              />
            )}
          </For>
        </Show>
      </div>

      <RmDialog
        open={scanOpen()}
        title={t("instanceDetail.mods.scanResult")}
        width={560}
        onClose={() => setScanOpen(false)}
        footer={
          <>
            <RmButton variant="ghost" onClick={() => setScanOpen(false)}>{t("common.cancel")}</RmButton>
            <Show
              when={(scan()?.report.missing_required.length ?? 0) > 0}
              fallback={<span class="muted-line">{t("instanceDetail.mods.noMissing")}</span>}
            >
              <RmButton variant="primary" icon="download" loading={applying()} onClick={() => void applyPlan()}>
                {t("instanceDetail.mods.applyPlan", { n: scan()!.report.missing_required.length })}
              </RmButton>
            </Show>
          </>
        }
      >
        <Show when={scan()}>
          {(data) => (
            <div class="scan-results">
              <div class="scan-section">
                <div class="scan-section-title">{t("instanceDetail.mods.scan.required", { n: data().report.missing_required.length })}</div>
                <For each={data().report.missing_required}>
                  {(dep) => (
                    <div class="scan-row">
                      <span class="scan-mod">{dep.mod_id}</span>
                      <Show when={dep.constraint}><RmTag>{dep.constraint}</RmTag></Show>
                      <span class="scan-required-by">{t("instanceDetail.mods.scan.requiredBy", { by: dep.required_by.join(", ") })}</span>
                    </div>
                  )}
                </For>
              </div>
              <div class="scan-section">
                {t("instanceDetail.mods.scan.conflicts", { n: data().report.conflicts.length })} ·{" "}
                {t("instanceDetail.mods.scan.unparsed", { n: data().report.unparsed.length })}
              </div>
            </div>
          )}
        </Show>
      </RmDialog>
    </div>
  );
}


// ── Tab 3 · Loaders ──


/** Loaders that conflict with an installed fabric-family loader (04 §5). */
function incompatibleWith(current: string): string[] {
  if (current === "fabric" || current === "legacy_fabric" || current === "quilt") {
    return ["forge", "neo_forge", "cleanroom", "opti_fine"];
  }
  if (current === "forge" || current === "neo_forge" || current === "cleanroom") {
    return ["fabric", "legacy_fabric", "quilt", "opti_fine"];
  }
  return [];
}

function LoadersTab(props: { profile: Profile }): JSX.Element {
  const [versionDialog, setVersionDialog] = createSignal<string | null>(null);
  const [versions_, setVersions_] = createSignal<LoaderVersion[]>([]);
  const [loadingVersions, setLoadingVersions] = createSignal(false);
  const [selectedVersion, setSelectedVersion] = createSignal("");
  const installing = createMemo(() => modloadersStore.forProfile(props.profile.id));
  const current = () => props.profile.mod_loader;

  async function openVersions(loaderType: string): Promise<void> {
    setVersionDialog(loaderType);
    const gv = props.profile.game_version;
    // 先回缓存秒显，再后台刷新（打开即视为一次更新请求）。
    const cached = versionCache.cachedLoaderVersions(loaderType, gv);
    if (cached) {
      setVersions_(cached);
      const firstCached = cached.find((v) => v.stable) ?? cached[0];
      setSelectedVersion(firstCached?.version ?? "");
      setLoadingVersions(false);
    } else {
      setLoadingVersions(true);
    }
    const res = await versionCache.refreshLoaderVersions(loaderType, gv);
    // 用户快速切换弹窗目标时，晚到的旧结果不覆盖新目标。
    if (versionDialog() !== loaderType) return;
    setLoadingVersions(false);
    if (res.length > 0) {
      setVersions_(res);
      const firstStable = res.find((v) => v.stable) ?? res[0];
      setSelectedVersion(firstStable?.version ?? "");
    } else if (!cached) {
      toast({ message: t("instanceDetail.loader.loadFailed"), tone: "error" });
    }
  }

  async function install(): Promise<void> {
    const loaderType = versionDialog();
    if (!loaderType || !selectedVersion()) return;
    setVersionDialog(null);
    const res = await modLoader.install(props.profile.id, loaderType, selectedVersion());
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
    else toast({ message: t("instanceDetail.loader.started"), tone: "info" });
  }

  return (
    <div class="tab-stack">
      <div class="loader-grid">
        <For each={LOADER_CARDS}>
          {(card) => {
            const isCurrent = () => current()?.loader_type === card.id;
            const disabledReason = createMemo(() =>
              current() ? (incompatibleWith(current()!.loader_type).includes(card.id) ? current()!.loader_type : null) : null
            );
            const installState = () => installing().find((i) => i.loaderType === card.id);
            return (
              <div class={`loader-card${isCurrent() ? " is-current" : ""}${disabledReason() ? " is-disabled" : ""}`}>
                <div class="loader-card-head">
                  <img class="loader-card-icon" src={loaderIcon(card.id) ?? ""} alt="" loading="lazy" />
                  <div class="loader-card-name">{card.label}</div>
                </div>
                <Show when={isCurrent()} fallback={<RmTag>{t("instanceDetail.loader.notInstalled")}</RmTag>}>
                  <RmTag tone="success">✓ {current()!.loader_version}</RmTag>
                </Show>
                <Show when={installState()}>
                  {(state) => <div class="loader-progress">{state().message} {state().percent}%</div>}
                </Show>
                <div class="loader-card-actions">
                  <Show
                    when={disabledReason()}
                    fallback={
                      <RmButton size="sm" disabled={isCurrent()} onClick={() => void openVersions(card.id)}>
                        {t("instanceDetail.loader.install")}
                      </RmButton>
                    }
                  >
                    {(reason) => <span class="muted-line">{t("instanceDetail.loader.incompatible", { current: reason() })}</span>}
                  </Show>
                </div>
              </div>
            );
          }}
        </For>
      </div>

      <RmDialog
        open={versionDialog() !== null}
        title={`${t("instanceDetail.loader.pickVersion")} · ${versionDialog() ?? ""}`}
        onClose={() => setVersionDialog(null)}
        footer={
          <>
            <RmButton variant="ghost" onClick={() => setVersionDialog(null)}>{t("common.cancel")}</RmButton>
            <RmButton variant="primary" disabled={!selectedVersion()} onClick={() => void install()}>
              {t("common.install")}
            </RmButton>
          </>
        }
      >
        <Show when={!loadingVersions()} fallback={<div class="muted-line">{t("common.loading")}</div>}>
          <div class="rm-list">
            <For each={versions_()}>
              {(version) => (
                <RmListRow
                  title={version.version}
                  selected={selectedVersion() === version.version}
                  onClick={() => setSelectedVersion(version.version)}
                  trailing={<RmTag tone={version.stable ? "success" : "warning"}>{version.stable ? t("instanceDetail.loader.stable") : t("instanceDetail.loader.beta")}</RmTag>}
                />
              )}
            </For>
          </div>
        </Show>
      </RmDialog>
    </div>
  );
}

// ── Tab 4 · Settings (inherit locks) ──

const OVERRIDABLE_FIELDS = [
  "java_path",
  "memory_mb",
  "min_memory_mb",
  "metaspace_mb",
  "width",
  "height",
  "fullscreen",
  "process_priority",
  "jvm_args",
  "game_args",
  "java_agents",
  "wrapper",
  "pre_launch_command",
  "post_exit_command",
  "use_native_glfw",
  "use_native_openal",
  "use_custom_natives",
  "enable_debug_log_output",
  "no_generated_jvm_args",
  "no_generated_optimizing_jvm_args",
] as const;

const ICON_MAX_BYTES = 512 * 1024;

/** Instance icon picker (profile.icon; stored as a data URL, "" clears). */
function InstanceIconCard(props: { profile: Profile }): JSX.Element {
  let fileInput: HTMLInputElement | undefined;
  const icon = () => props.profile.icon;

  async function saveIcon(dataUrl: string): Promise<void> {
    const res = await profiles.update(props.profile.id, { icon: dataUrl });
    if (res.ok) {
      profilesStore.upsert(res.data);
      toast({ message: t("common.saved"), tone: "success" });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  function onPick(files: FileList | null): void {
    const file = files?.[0];
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      toast({ message: t("instanceDetail.settings.icon.notImage"), tone: "error" });
      return;
    }
    if (file.size > ICON_MAX_BYTES) {
      toast({ message: t("instanceDetail.settings.icon.tooLarge"), tone: "error" });
      return;
    }
    const reader = new FileReader();
    reader.onload = () => void saveIcon(String(reader.result));
    reader.onerror = () => toast({ message: t("instanceDetail.settings.icon.readFail"), tone: "error" });
    reader.readAsDataURL(file);
    // Allow re-picking the same file later.
    if (fileInput) fileInput.value = "";
  }

  return (
    <RmCard title={t("instanceDetail.settings.icon.title")}>
      <div class="btn-row">
        <Show when={icon()} fallback={<div class="instance-icon-slot"><Icon name="package" size={22} /></div>}>
          <img class="instance-icon-preview" src={icon()!} alt="" />
        </Show>
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          class="visually-hidden"
          onChange={(e) => onPick(e.currentTarget.files)}
        />
        <RmButton size="sm" icon="plus" onClick={() => fileInput?.click()}>
          {icon() ? t("instanceDetail.settings.icon.replace") : t("instanceDetail.settings.icon.pick")}
        </RmButton>
        <Show when={icon()}>
          <RmButton size="sm" variant="ghost" icon="trash" onClick={() => void saveIcon("")}>
            {t("instanceDetail.settings.icon.clear")}
          </RmButton>
        </Show>
      </div>
      <div class="muted-line">{t("instanceDetail.settings.icon.hint")}</div>
    </RmCard>
  );
}

function SettingsTab(props: { profile: Profile }): JSX.Element {
  const [resolved, setResolved] = createSignal<ResolvedInstanceSettings | null>(null);
  const [javas, setJavas] = createSignal<JavaInfo[]>([]);
  const [draftOverrides, setDraftOverrides] = createSignal<Set<string>>(
    new Set(props.profile.override_properties)
  );
  const [dirty, setDirty] = createSignal(false);

  // Local draft of the overridable values (initialized from the profile; the
  // displayed value for inherited fields comes from `resolved`).
  const [values, setValues] = createSignal<Record<string, unknown>>({
    java_path: props.profile.java_path ?? "",
    memory_mb: props.profile.memory_mb,
    min_memory_mb: props.profile.min_memory_mb,
    metaspace_mb: props.profile.metaspace_mb,
    width: props.profile.width ?? 854,
    height: props.profile.height ?? 480,
    fullscreen: props.profile.fullscreen,
    process_priority: props.profile.process_priority,
    jvm_args: props.profile.jvm_args.join("\n"),
    game_args: props.profile.game_args.join("\n"),
    java_agents: props.profile.java_agents.join("\n"),
    wrapper: props.profile.wrapper ?? "",
    pre_launch_command: props.profile.pre_launch_command ?? "",
    post_exit_command: props.profile.post_exit_command ?? "",
    use_native_glfw: props.profile.use_native_glfw,
    use_native_openal: props.profile.use_native_openal,
    use_custom_natives: props.profile.use_custom_natives,
    enable_debug_log_output: props.profile.enable_debug_log_output,
    no_generated_jvm_args: props.profile.no_generated_jvm_args,
    no_generated_optimizing_jvm_args: props.profile.no_generated_optimizing_jvm_args,
  });

  onMount(() => {
    void instances.resolvedSettings(props.profile.id).then((r) => r.ok && setResolved(r.data));
    void settingsApi.detectJava().then((r) => r.ok && setJavas(r.data));
  });

  const overridden = (field: string): boolean => draftOverrides().has(field);
  const effective = (field: string): unknown => {
    if (overridden(field)) return values()[field];
    return (resolved() as unknown as Record<string, unknown>)?.[field];
  };

  function toggleLock(field: string): void {
    setDraftOverrides((set) => {
      const next = new Set(set);
      if (next.has(field)) next.delete(field);
      else next.add(field);
      return next;
    });
    setDirty(true);
  }

  function setValue(field: string, value: unknown): void {
    setValues((v) => ({ ...v, [field]: value }));
    setDirty(true);
  }

  async function save(): Promise<void> {
    const v = values();
    const lines = (key: string) =>
      String(v[key] ?? "")
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean);
    const opts: ProfileUpdateOpts = {
      override_properties: [...draftOverrides()],
    };
    for (const field of draftOverrides()) {
      switch (field) {
        case "java_path": opts.java_path = String(v.java_path) || null; break;
        case "memory_mb": opts.memory_mb = Number(v.memory_mb); break;
        case "min_memory_mb": opts.min_memory_mb = Number(v.min_memory_mb); break;
        case "metaspace_mb": opts.metaspace_mb = Number(v.metaspace_mb); break;
        case "width": opts.width = Number(v.width); break;
        case "height": opts.height = Number(v.height); break;
        case "fullscreen": opts.fullscreen = Boolean(v.fullscreen); break;
        case "process_priority": opts.process_priority = String(v.process_priority); break;
        case "jvm_args": opts.jvm_args = lines("jvm_args"); break;
        case "game_args": opts.game_args = lines("game_args"); break;
        case "java_agents": opts.java_agents = lines("java_agents"); break;
        case "wrapper": opts.wrapper = String(v.wrapper) || null; break;
        case "pre_launch_command": opts.pre_launch_command = String(v.pre_launch_command) || null; break;
        case "post_exit_command": opts.post_exit_command = String(v.post_exit_command) || null; break;
        case "use_native_glfw": opts.use_native_glfw = Boolean(v.use_native_glfw); break;
        case "use_native_openal": opts.use_native_openal = Boolean(v.use_native_openal); break;
        case "use_custom_natives": opts.use_custom_natives = Boolean(v.use_custom_natives); break;
        case "enable_debug_log_output": opts.enable_debug_log_output = Boolean(v.enable_debug_log_output); break;
        case "no_generated_jvm_args": opts.no_generated_jvm_args = Boolean(v.no_generated_jvm_args); break;
        case "no_generated_optimizing_jvm_args": opts.no_generated_optimizing_jvm_args = Boolean(v.no_generated_optimizing_jvm_args); break;
      }
    }
    const res = await profiles.update(props.profile.id, opts);
    if (res.ok) {
      profilesStore.upsert(res.data);
      setDirty(false);
      toast({ message: t("common.saved"), tone: "success" });
      void instances.resolvedSettings(props.profile.id).then((r) => r.ok && setResolved(r.data));
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  const lock = (field: string) => (
    <RmInheritLock overridden={overridden(field)} onToggle={() => toggleLock(field)} />
  );
  const subtitle = (field: string) => (
    <Show
      when={overridden(field)}
      fallback={
        <span class="muted-line">
          {t("instanceDetail.settings.inherit", { value: String(effective(field) ?? "—") })}
        </span>
      }
    >
      <span class="muted-line">{t("instanceDetail.settings.overridden")}</span>
    </Show>
  );

  return (
    <div class="tab-stack settings-stack">
      <InstanceIconCard profile={props.profile} />

      <RmCard title={t("instanceDetail.settings.runtime")}>
        <RmFieldRow label={t("instanceDetail.settings.java")} hint={subtitle("java_path")}>
          <div class="btn-row">
            {lock("java_path")}
            <RmSelect
              value={overridden("java_path") ? String(values().java_path ?? "") : ""}
              disabled={!overridden("java_path")}
              onChange={(v) => setValue("java_path", v)}
              options={[
                { value: "", label: t("instanceDetail.settings.javaAuto") },
                ...javas().map((j) => ({ value: j.path, label: `Java ${j.major_version} · ${j.vendor}${j.is_64bit ? " · 64-bit" : ""}` })),
              ]}
            />
          </div>
        </RmFieldRow>
        <RmFieldRow label={t("instanceDetail.settings.maxMemory")} hint={subtitle("memory_mb")}>
          <div class="btn-row">
            {lock("memory_mb")}
            <input
              class="rm-input"
              type="number"
              min={512}
              max={32768}
              step={512}
              disabled={!overridden("memory_mb")}
              value={Number(overridden("memory_mb") ? values().memory_mb : effective("memory_mb"))}
              onInput={(e) => setValue("memory_mb", Number(e.currentTarget.value))}
            />
          </div>
        </RmFieldRow>
      </RmCard>

      <RmCard title={t("instanceDetail.settings.window")}>
        <RmFieldRow label={t("instanceDetail.settings.resolution")} hint={subtitle("width")}>
          <div class="btn-row">
            {lock("width")}
            <input class="rm-input" type="number" style={{ width: "100px" }} disabled={!overridden("width")} value={Number(overridden("width") ? values().width : effective("width"))} onInput={(e) => setValue("width", Number(e.currentTarget.value))} />
            ×
            <input class="rm-input" type="number" style={{ width: "100px" }} disabled={!overridden("width")} value={Number(overridden("width") ? values().height : effective("height"))} onInput={(e) => setValue("height", Number(e.currentTarget.value))} />
          </div>
        </RmFieldRow>
        <RmFieldRow label={t("instanceDetail.settings.fullscreen")} hint={subtitle("fullscreen")}>
          <div class="btn-row">
            {lock("fullscreen")}
            <RmSwitch checked={Boolean(overridden("fullscreen") ? values().fullscreen : effective("fullscreen"))} disabled={!overridden("fullscreen")} onChange={(v) => setValue("fullscreen", v)} />
          </div>
        </RmFieldRow>
        <RmFieldRow label={t("instanceDetail.settings.priority")} hint={subtitle("process_priority")}>
          <div class="btn-row">
            {lock("process_priority")}
            <RmSelect
              value={overridden("process_priority") ? String(values().process_priority) : String(effective("process_priority") ?? "normal")}
              disabled={!overridden("process_priority")}
              onChange={(v) => setValue("process_priority", v)}
              options={[
                { value: "low", label: "low" },
                { value: "below_normal", label: "below_normal" },
                { value: "normal", label: "normal" },
                { value: "above_normal", label: "above_normal" },
                { value: "high", label: "high" },
              ]}
            />
          </div>
        </RmFieldRow>
      </RmCard>

      <RmAdvancedSection title={t("instanceDetail.settings.advancedArgs")} summary={t("instanceDetail.settings.argsSummary")}>
        <RmFieldRow label={t("instanceDetail.settings.jvmArgs")} hint={subtitle("jvm_args")}>
          <div class="btn-row">{lock("jvm_args")}</div>
          <textarea class="rm-textarea rm-input-mono" rows={3} disabled={!overridden("jvm_args")} value={String(overridden("jvm_args") ? values().jvm_args : effective("jvm_args") ?? "")} onInput={(e) => setValue("jvm_args", e.currentTarget.value)} />
        </RmFieldRow>
        <RmFieldRow label={t("instanceDetail.settings.gameArgs")} hint={subtitle("game_args")}>
          <div class="btn-row">{lock("game_args")}</div>
          <textarea class="rm-textarea rm-input-mono" rows={2} disabled={!overridden("game_args")} value={String(overridden("game_args") ? values().game_args : effective("game_args") ?? "")} onInput={(e) => setValue("game_args", e.currentTarget.value)} />
        </RmFieldRow>
        <RmFieldRow label={t("instanceDetail.settings.wrapper")} hint={subtitle("wrapper")}>
          <div class="btn-row">{lock("wrapper")}</div>
          <input class="rm-input rm-input-mono" disabled={!overridden("wrapper")} value={String(overridden("wrapper") ? values().wrapper : effective("wrapper") ?? "")} onInput={(e) => setValue("wrapper", e.currentTarget.value)} />
        </RmFieldRow>
      </RmAdvancedSection>

      <RmAdvancedSection title={t("instanceDetail.settings.advancedDebug")}>
        <For each={["use_native_glfw", "use_native_openal", "use_custom_natives", "enable_debug_log_output", "no_generated_jvm_args", "no_generated_optimizing_jvm_args"] as const}>
          {(field) => (
            <RmFieldRow label={t(`instanceDetail.settings.field.${field}`)} hint={subtitle(field)}>
              <div class="btn-row">
                {lock(field)}
                <RmSwitch
                  checked={Boolean(overridden(field) ? values()[field] : effective(field))}
                  disabled={!overridden(field)}
                  onChange={(v) => setValue(field, v)}
                />
              </div>
            </RmFieldRow>
          )}
        </For>
      </RmAdvancedSection>

      <div class="settings-footer">
        <RmButton variant="primary" disabled={!dirty()} onClick={() => void save()}>
          {t("common.save")}
        </RmButton>
      </div>
    </div>
  );
}

// ── Tab 5 · Maintenance ──

function MaintenanceTab(props: { profile: Profile }): JSX.Element {
  const [missing, setMissing] = createSignal<MissingFileInfo[] | null>(null);
  const [checking, setChecking] = createSignal(false);
  const [repairing, setRepairing] = createSignal(false);
  const [isolation, setIsolation] = createSignal(props.profile.isolation ?? "never");
  const [paths, setPaths] = createSignal<InstancePaths | null>(null);
  const [deleteOpen, setDeleteOpen] = createSignal(false);

  createEffect(() => {
    void instances.paths(props.profile.id).then((r) => r.ok && setPaths(r.data));
  });

  async function check(): Promise<void> {
    setChecking(true);
    const res = await instances.checkCompletion(props.profile.id);
    setChecking(false);
    if (res.ok) setMissing(res.data);
    else toast({ message: res.error.message, tone: "error" });
  }

  async function repair(): Promise<void> {
    setRepairing(true);
    const res = await instances.repair(props.profile.id);
    setRepairing(false);
    if (res.ok) {
      toast({ message: t("instanceDetail.maintenance.repaired", { n: res.data }), tone: "success" });
      void check();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  const running = () => launchStore.isRunning(props.profile.id);

  return (
    <div class="tab-stack">
      <RmCard title={t("instanceDetail.maintenance.completion")}>
        <div class="btn-row">
          <RmButton icon="shield" loading={checking()} onClick={() => void check()}>
            {t("instanceDetail.maintenance.check")}
          </RmButton>
          <Show when={missing() && missing()!.length > 0}>
            <RmButton variant="primary" icon="wrench" loading={repairing()} onClick={() => void repair()}>
              {t("instanceDetail.maintenance.repair", { n: missing()!.length })}
            </RmButton>
          </Show>
        </div>
        <Show when={missing()}>
          {(list) => (
            <div class="rm-list">
              <For each={list()}>
                {(file) => (
                  <RmListRow
                    icon={<Icon name="alert" size={16} class="text-warning" />}
                    title={file.path}
                    subtitle={`${file.kind} · ${file.description}`}
                  />
                )}
              </For>
            </div>
          )}
        </Show>
      </RmCard>

      <RmCard title={t("instanceDetail.maintenance.isolation")} actions={<Show when={running()}><RmTag tone="warning">{t("instances.running")}</RmTag></Show>}>
        <div class="btn-row">
          <RmSelect
            value={isolation()}
            disabled={running()}
            onChange={(v) => setIsolation(v)}
            options={[
              { value: "never", label: t("instanceDetail.maintenance.isolationShared") },
              { value: "always", label: t("instanceDetail.maintenance.isolationIsolated") },
              { value: "modded", label: t("instanceDetail.maintenance.isolationModded") },
            ]}
          />
          <RmButton
            size="sm"
            disabled={running() || isolation() === (props.profile.isolation ?? "never")}
            onClick={async () => {
              const res = await profiles.update(props.profile.id, { isolation: isolation() });
              if (res.ok) {
                profilesStore.upsert(res.data);
                void instances.paths(props.profile.id).then((r) => r.ok && setPaths(r.data));
                toast({ message: t("common.saved"), tone: "success" });
              } else toast({ message: res.error.message, tone: "error" });
            }}
          >
            {t("common.save")}
          </RmButton>
        </div>
        <div class="muted-line">{t("instanceDetail.maintenance.isolationWarning")}</div>
        <Show when={paths()}>
          <div class="path-row">
            <span class="path-row-label">{t("instanceDetail.browse.runDir")}</span>
            <span class="path-row-value" title={paths()!.run_dir}>{elidePath(paths()!.run_dir)}</span>
          </div>
        </Show>
      </RmCard>

      <RmCard title={t("instanceDetail.maintenance.danger")} danger>
        <div class="btn-row">
          <RmButton icon="edit" onClick={() => void renameProfile(props.profile)}>
            {t("instances.menu.rename")}
          </RmButton>
          <RmButton variant="danger" icon="trash" disabled={running()} onClick={() => setDeleteOpen(true)}>
            {t("instances.menu.delete")}
          </RmButton>
        </div>
      </RmCard>

      <DeleteInstanceDialog profile={props.profile} open={deleteOpen()} onClose={() => setDeleteOpen(false)} />
    </div>
  );
}

