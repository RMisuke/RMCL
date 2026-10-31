/**
 * Instance detail (ui-design 04): five hash-tabs (overview / mods / loaders /
 * settings / maintenance), side toolbar with launch + browse/manage menus,
 * field-level inherit locks, dependency scan + one-click apply (pure logic).
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, onCleanup, Switch, Match, on } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { DeleteInstanceDialog } from "../components/DeleteInstanceDialog";
import { Icon } from "../components/icons";
import { RmButton, RmIconButton, RmInput, RmTag, RmCheckbox } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import {
  RmCard,
  RmListRow,
  RmSideTabs,
  RmEmptyState,
  RmErrorState,
} from "../components/rm/surfaces";
import { instances, mods, modLoader } from "../ipc/api";
import type {
  InstanceDetail as InstanceDetailData,
  ResolvedInstanceSettings,
  LoaderVersion,
  ModsDependencyScanView,
  AppliedPlanReport,
} from "../ipc/types";
import { instancesStore } from "../stores/instances";
import { toInstanceView, toSettingsPatch, type InstanceView } from "../lib/instanceView";
import { InstanceSettings } from "../components/instance-settings/InstanceSettings";
import { launchStore } from "../stores/launch";
import { tasksStore } from "../stores/tasks";
import { uiStore, toast, promptText } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { relativeTime } from "../lib/format";
import { LOADER_CARDS, loaderLabel } from "../lib/loaders";
import { INSTANCE_FOLDERS, type InstanceFolder } from "../lib/instanceFolders";
import { loaderIcon } from "../lib/mcIcons";
import * as versionCache from "../stores/versionCache";
import { useHashTab } from "../lib/useHashTab";

const TABS: { id: string; icon: "list" | "package" | "layers" | "sliders"; label: TranslationKey }[] = [
  { id: "overview", icon: "list", label: "instanceDetail.tab.overview" },
  { id: "mods", icon: "package" as const, label: "instanceDetail.tab.mods" },
  { id: "loaders", icon: "layers" as const, label: "instanceDetail.tab.loaders" },
  { id: "settings", icon: "sliders" as const, label: "instanceDetail.tab.settings" },
];

export default function InstanceDetail(): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const [tab, setTab] = useHashTab("overview", TABS.map((x) => x.id));

  const profileId = () => {
    const raw = params.id ?? "";
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  };
  const [loaded, setLoaded] = createSignal<InstanceDetailData | null>(null);
  const [loadError, setLoadError] = createSignal<string | null>(null);
  const detail = createMemo(
    () => instancesStore.get(profileId()) ?? loaded()
  );
  const profile = createMemo(() => {
    const data = detail();
    return data ? toInstanceView(data) : null;
  });

  async function load(): Promise<void> {
    const res = await instances.get(profileId());
    if (res.ok) {
      setLoaded(res.data);
      instancesStore.upsert(res.data);
      setLoadError(null);
    } else {
      setLoadError(res.error.message);
    }
  }

  onMount(() => void load());

  // Settings are auto-saved on change (no explicit Save button), so tabs can
  // switch freely without an unsaved-draft confirmation.
  const changeTab = setTab;

  async function launch(): Promise<void> {
    const res = await launchStore.start(profileId(), profile()?.name);
    if (!res.ok) toast({ message: res.message ?? "", tone: "error" });
  }

  return (
    <div class="page">
      <PageHeader
        crumbs={[
          { label: t("instances.title"), href: "/instances" },
          ...(detail()?.directory.name
            ? [{ label: detail()!.directory.name, href: "/instances" }]
            : []),
        ]}
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
              <RmButton size="sm" variant="danger" icon="square" onClick={() => void launchStore.stop(profileId())}>
                {t("common.stop")}
              </RmButton>
              <RmButton size="sm" icon="terminal" onClick={() => uiStore.openLogDrawer(profile()?.id ?? null)}>
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
                onSelect={changeTab}
                footer={
                  <div class="instance-toolbar">
                    <RmButton size="sm" icon={launchStore.isRunning(p().id) ? "square" : "play"} variant={launchStore.isRunning(p().id) ? "danger" : "primary"} onClick={() => (launchStore.isRunning(p().id) ? void launchStore.stop(p().id) : void launch())}>
                      {launchStore.isRunning(p().id) ? t("common.stop") : t("common.launch")}
                    </RmButton>
                    <BrowseMenu profile={p()} />
                    <ManageMenu profile={p()} />
                  </div>
                }
              />
              <div class="side-tab-content">
                <Switch>
                  <Match when={tab() === "overview"}><OverviewTab profile={p()} onGoTab={changeTab} /></Match>
                  <Match when={tab() === "mods"}><ModsTab profile={p()} /></Match>
                  <Match when={tab() === "loaders"}><LoadersTab profile={p()} /></Match>
                  <Match when={tab() === "settings"}><InstanceSettings detail={detail()!} /></Match>
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

async function revealInstanceFolder(key: string, folder: InstanceFolder): Promise<void> {
  const res = await instances.reveal(key, folder);
  if (!res.ok) toast({ message: res.error.message, tone: "error" });
}

function BrowseMenu(props: { profile: InstanceView }): JSX.Element {
  return (
    <RmMenu
      portal
      items={INSTANCE_FOLDERS.map((folder) => ({
        label: t(folder.labelKey),
        icon: folder.icon,
        onSelect: () => void revealInstanceFolder(props.profile.id, folder.id),
      }))}
    >
      <RmButton size="sm" icon="folder">
        {t("instanceDetail.browse")}
      </RmButton>
    </RmMenu>
  );
}

function ManageMenu(props: { profile: InstanceView }): JSX.Element {
  const navigate = useNavigate();
  const [open, setOpen] = createSignal(false);
  const [deleteOpen, setDeleteOpen] = createSignal(false);
  return (
    <>
      <RmButton size="sm" icon="more-horizontal" onClick={() => setOpen(true)}>
        {t("instanceDetail.manage")}
      </RmButton>
      <RmDialog
        open={open()}
        title={t("instanceDetail.manage")}
        width={420}
        onClose={() => setOpen(false)}
        footer={<RmButton onClick={() => setOpen(false)}>{t("common.close")}</RmButton>}
      >
        <div class="instance-manage-actions">
          <RmButton
            icon="edit"
            onClick={() => {
              setOpen(false);
              void renameInstanceView(props.profile);
            }}
          >
            {t("instances.menu.rename")}
          </RmButton>
          <RmButton
            icon="package"
            onClick={() => {
              setOpen(false);
              navigate(`/download?instance=${props.profile.id}#mods`);
            }}
          >
            {t("instances.menu.browseMods")}
          </RmButton>
          <RmButton
            variant="danger"
            icon="trash"
            onClick={() => {
              setOpen(false);
              setDeleteOpen(true);
            }}
          >
            {t("instances.menu.delete")}
          </RmButton>
        </div>
      </RmDialog>
      <DeleteInstanceDialog
        instance={{ key: props.profile.id, name: props.profile.name, external: props.profile.external_root }}
        open={deleteOpen()}
        onClose={() => setDeleteOpen(false)}
      />
    </>
  );
}

async function renameInstanceView(profile: InstanceView): Promise<void> {
  // WKWebView 不支持 window.prompt —— 统一走 prompt 对话框。
  const next = await promptText({
    title: t("instances.menu.rename"),
    label: t("instances.rename.prompt"),
    initial: profile.name,
  });
  if (!next || next === profile.name) return;
  const res = await updateInstance(profile.id, { name: next });
  if (res.ok) {
    instancesStore.upsert(res.data);
    toast({ message: t("instances.renamed"), tone: "success" });
  } else {
    toast({ message: res.error.message, tone: "error" });
  }
}


async function updateInstance(
  key: string,
  opts: Record<string, unknown>
): Promise<{ ok: true; data: InstanceDetailData } | { ok: false; error: { message: string } }> {
  const res = await instances.updateSettings(key, toSettingsPatch(opts));
  if (res.ok) instancesStore.upsert(res.data);
  return res as
    | { ok: true; data: InstanceDetailData }
    | { ok: false; error: { message: string } };
}

// ── Tab 1 · Overview ──

function OverviewTab(props: { profile: InstanceView; onGoTab: (tab: string) => void }): JSX.Element {
  const navigate = useNavigate();
  const [resolved, setResolved] = createSignal<ResolvedInstanceSettings | null>(null);

  // Key on the profile id, not the object (perf audit 2.5).
  createEffect(
    on(
      () => props.profile.id,
      (id) => {
        void instances.resolvedSettings(id).then((r) => {
          if (r.ok) setResolved(r.data);
          else toast({ message: r.error.message, tone: "error" });
        });
      }
    )
  );

  return (
    <div class="tab-stack">
      <RmCard title={t("instanceDetail.overview.identity")}>
        <div class="chip-row">
          <RmTag tone="accent">{props.profile.game_version}</RmTag>
          <Show when={props.profile.mod_loader}>
            <RmTag tone="accent">{loaderLabel(props.profile.mod_loader!.loader_type)} {props.profile.mod_loader!.loader_version}</RmTag>
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
        <div class="instance-folder-grid">
          <For each={INSTANCE_FOLDERS}>
            {(folder) => (
              <button
                class="instance-folder-btn"
                type="button"
                title={t(folder.labelKey)}
                onClick={() => void revealInstanceFolder(props.profile.id, folder.id)}
              >
                <span class="instance-folder-btn-icon">
                  <Icon name={folder.icon} size={18} />
                </span>
                <span>{t(folder.labelKey)}</span>
              </button>
            )}
          </For>
        </div>
        <div class="muted-line">{t("instanceDetail.folders.hint")}</div>
      </RmCard>

      <RmCard title={t("instanceDetail.overview.quickActions")}>
        <div class="btn-row">
          <RmButton icon="package" onClick={() => navigate(`/download?instance=${props.profile.id}#mods`)}>
            {t("instanceDetail.overview.goMods")}
          </RmButton>
          <RmButton icon="shield" onClick={() => props.onGoTab("mods")}>
            {t("instanceDetail.mods.missingScan")}
          </RmButton>
          <RmButton icon="sliders" onClick={() => props.onGoTab("settings")}>
            {t("instanceDetail.tab.settings")}
          </RmButton>
        </div>
      </RmCard>
    </div>
  );
}

// ── Tab 2 · Mods management ──

function ModsTab(props: { profile: InstanceView }): JSX.Element {
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
  // The search filter applies to disabled mods too (P2-20).
  const filteredDisabled = createMemo(() =>
    disabled().filter((f) => f.toLowerCase().includes(search().toLowerCase()))
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
    const res: { ok: boolean; data?: AppliedPlanReport; message?: string } = await mods
      .applyPlan(props.profile.id)
      .then((r) => (r.ok ? { ok: true, data: r.data } : { ok: false, message: r.error.message }));
    setApplying(false);
    setScanOpen(false);
    if (res.ok && res.data) {
      toast({
        message: t("instanceDetail.mods.applied", { n: res.data.installed.length }),
        tone: "success",
      });
      void refreshMods();
    } else {
      toast({ message: res.message ?? "", tone: "error" });
    }
  }

  return (
    <div class="tab-stack">
      <div class="toolbar">
        <RmButton size="sm" icon="refresh" onClick={() => void refreshMods()}>{t("common.refresh")}</RmButton>
        <RmButton size="sm" icon="plus" onClick={() => void addLocal()}>{t("instanceDetail.mods.addLocal")}</RmButton>
        <RmButton size="sm" icon="package" onClick={() => navigate(`/download?instance=${props.profile.id}#mods`)}>
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
          <For each={filteredDisabled()}>
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
              <Show when={data().report.conflicts.length > 0}>
                <div class="scan-section">
                  <div class="scan-section-title">{t("instanceDetail.mods.scan.conflicts", { n: data().report.conflicts.length })}</div>
                  <For each={data().report.conflicts}>
                    {(conflict) => (
                      <div class="scan-row">
                        <span class="scan-mod">{conflict.mod_id}</span>
                        <span class="scan-required-by">{t("instanceDetail.mods.scan.requiredBy", { by: conflict.declared_by })}</span>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
              <Show when={data().report.unparsed.length > 0}>
                <div class="scan-section">
                  <div class="scan-section-title">{t("instanceDetail.mods.scan.unparsed", { n: data().report.unparsed.length })}</div>
                  <For each={data().report.unparsed}>
                    {(item) => (
                      <div class="scan-row">
                        <span class="scan-mod">{String(item)}</span>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
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

function LoadersTab(props: { profile: InstanceView }): JSX.Element {
  const navigate = useNavigate();
  const [versionDialog, setVersionDialog] = createSignal<string | null>(null);
  const [versions_, setVersions_] = createSignal<LoaderVersion[]>([]);
  const [loadingVersions, setLoadingVersions] = createSignal(false);
  const [selectedVersion, setSelectedVersion] = createSignal("");
  const installing = createMemo(() =>
    tasksStore.forTarget(props.profile.id).filter((task) => task.kind === "modloader-install")
  );
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
      // Keep the user's current pick when it still exists (N-10, mirrors the
      // wizard's applyLoaderVersions); fall back to the newest stable.
      const current = selectedVersion();
      setSelectedVersion(
        current && res.some((v) => v.version === current)
          ? current
          : (res.find((v) => v.stable) ?? res[0])?.version ?? ""
      );
    } else if (!cached) {
      toast({ message: t("instanceDetail.loader.loadFailed"), tone: "error" });
    }
  }

  async function install(): Promise<void> {
    const loaderType = versionDialog();
    if (!loaderType || !selectedVersion()) return;
    setVersionDialog(null);
    const res = await modLoader.install(props.profile.id, loaderType, selectedVersion());
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    toast({ message: t("instanceDetail.loader.started"), tone: "info" });
    await instancesStore.refresh(true);
    navigate(`/instances/${encodeURIComponent(res.data)}#loaders`);
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
            const installState = () => installing().find((task) => task.tag === card.id);
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
                  {(state) => <div class="loader-progress">{state().currentStepLabel ?? state().message} {Math.round(state().overallPercent)}%</div>}
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
