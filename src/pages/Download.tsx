/**
 * Download center (ui-design 05): install wizard (version → loader → info →
 * execute), Modrinth store with instance context (locked filters + promoted
 * recommended version), and the modpack placeholder.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, createEffect, onMount, Switch, Match } from "solid-js";
import { useNavigate, useSearchParams } from "@solidjs/router";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmTag, RmProgress } from "../components/rm/controls";
import { RmDialog } from "../components/rm/overlays";
import { RmSideTabs, RmListRow, RmEmptyState, RmErrorState, RmCard, RmFieldRow, RmSkeleton } from "../components/rm/surfaces";
import { VirtualList } from "../components/rm/VirtualList";
import { versions, instances, gameDirectories, modLoader, mods, external } from "../ipc/api";
import type {
  VersionInfo,
  ReleaseType,
  LoaderVersion,
  ModInfo,
  ModVersion,
  ModSearchResult,
  InstanceDetail,
} from "../ipc/types";
import { instancesStore } from "../stores/instances";
import * as versionCache from "../stores/versionCache";
import { cacheStore } from "../stores/cache";
import { tasksStore } from "../stores/tasks";
import { modpacksStore } from "../stores/modpacks";
import { uiStore, toast } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { abbreviateCount, formatBytes, formatDate, initialOf } from "../lib/format";
import { versionTypeIcon, loaderIcon } from "../lib/mcIcons";
import { LOADER_CARDS, loaderLabel, normalizeLoaderId } from "../lib/loaders";
import { useHashTab } from "../lib/useHashTab";
import { Icon } from "../components/icons";
import {
  filterModVersions,
  groupModVersions,
  isSnapshotGameVersion,
  sortGameVersionsDesc,
  versionChannel,
  type VersionChannel,
} from "../lib/modVersions";

const TABS: { id: string; icon: "download" | "package" | "layers"; label: TranslationKey }[] = [
  { id: "install", icon: "download", label: "download.tab.install" },
  { id: "mods", icon: "package" as const, label: "download.tab.mods" },
  { id: "modpack", icon: "layers" as const, label: "download.tab.modpack" },
];

/** Loaders that Modrinth exposes as search facets (instances still use `neo_forge`). */
const MOD_LOADER_FILTERS = [
  { id: "fabric", label: "Fabric" },
  { id: "neo_forge", label: "NeoForge" },
  { id: "forge", label: "Forge" },
  { id: "quilt", label: "Quilt" },
  { id: "lite_loader", label: "LiteLoader" },
];

type ModSort = "downloads" | "relevance" | "updated" | "newest";
const MOD_SORTS: { value: ModSort; label: TranslationKey }[] = [
  { value: "downloads", label: "modStore.sort.downloads" },
  { value: "relevance", label: "modStore.sort.relevance" },
  { value: "updated", label: "modStore.sort.updated" },
  { value: "newest", label: "modStore.sort.newest" },
];

function channelLabel(channel: VersionChannel): string {
  return t(`modStore.version.${channel}` as TranslationKey);
}

function channelTagClass(channel: VersionChannel): string {
  return `version-channel-label is-${channel}`;
}

export default function Download(): JSX.Element {
  const [tab, setTab] = useHashTab("install", TABS.map((x) => x.id));

  return (
    <div class="page">
      <PageHeader title={t("download.title")} />
      <div class="page-body page-with-sidetabs">
        <RmSideTabs
          items={TABS.map((x) => ({ ...x, label: t(x.label) }))}
          active={tab()}
          onSelect={setTab}
        />
        <div class="side-tab-content">
          <Switch>
            <Match when={tab() === "install"}><InstallWizard /></Match>
            <Match when={tab() === "mods"}><ModStore /></Match>
            <Match when={tab() === "modpack"}>
              <ModpackPanel />
            </Match>
          </Switch>
        </div>
      </div>
    </div>
  );
}

// ── Modpack panel ──

function ModpackPanel(): JSX.Element {
  const tasks = createMemo(() => tasksStore.byKind("modpack-install"));
  const modpacks = createMemo(() =>
    (instancesStore.items() ?? []).filter((item) => item.summary.is_modpack)
  );

  return (
    <div class="modpack-panel">
      <RmCard
        title={t("download.modpack.title")}
        actions={
          <RmButton size="sm" variant="primary" icon="plus" onClick={() => void modpacksStore.pickAndInspect()}>
            {t("modpack.pick")}
          </RmButton>
        }
      >
        <div class="modpack-hero">
          <div class="modpack-hero-icon"><Icon name="layers" size={34} /></div>
          <div class="modpack-hero-copy">
            <h4>{t("download.modpack.heading")}</h4>
            <p>{t("download.modpack.hint")}</p>
            <div class="modpack-format-tags">
              <For each={["CurseForge", "Modrinth", "MCBBS", "MultiMC / Prism", "HMCL", "Server", "Manual"]}>
                {(name) => <RmTag tone="neutral">{name}</RmTag>}
              </For>
            </div>
          </div>
        </div>
        <div class="modpack-drop-hint">
          <Icon name="download" size={15} />
          {t("download.modpack.dropHint")}
        </div>
      </RmCard>

      <Show when={tasks().length > 0}>
        <RmCard title={t("modpack.installingTitle")}>
          <For each={tasks()}>
            {(task) => (
              <div class="modpack-task-row">
                <div class="modpack-task-main">
                  <div class="modpack-task-title">{task.currentStepLabel ?? task.title}</div>
                  <div class="modpack-task-meta">
                    {Math.round(task.overallPercent)}%
                    <Show when={task.download && task.download.totalFiles > 0}>
                      {" · "}{task.download!.completedFiles}/{task.download!.totalFiles}
                    </Show>
                  </div>
                </div>
                <RmProgress value={task.overallPercent} />
                <RmButton size="sm" variant="ghost" onClick={() => void tasksStore.cancel(task.id)}>
                  {t("modpack.cancel")}
                </RmButton>
              </div>
            )}
          </For>
        </RmCard>
      </Show>

      <Show when={modpacks().length > 0}>
        <RmCard title={t("download.modpack.installed")}>
          <For each={modpacks().slice(0, 8)}>
            {(item) => (
              <div class="modpack-installed-row">
                <Icon name="layers" size={18} />
                <div class="modpack-installed-copy">
                  <div>{item.summary.display_name}</div>
                  <div class="modpack-installed-meta">
                    {item.summary.game_version}
                    <Show when={item.summary.loader}> · {item.summary.loader!.kind}</Show>
                  </div>
                </div>
              </div>
            )}
          </For>
        </RmCard>
      </Show>
    </div>
  );
}

// ── Install wizard ──

function InstallWizard(): JSX.Element {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [step, setStep] = createSignal(1);
  const [versions_, setVersions_] = createSignal<VersionInfo[] | null>(null);
  // Offline fallback: versions already on disk, picked via the advanced dialog.
  const [localVersions, setLocalVersions] = createSignal<InstanceDetail[]>([]);
  const [bindLocalOpen, setBindLocalOpen] = createSignal(false);
  const [typeFilter, setTypeFilter] = createSignal<"release" | "snapshot" | "old" | "all">("release");
  type VersionTypeFilter = "release" | "snapshot" | "old" | "all";
  const [search, setSearch] = createSignal(String(searchParams.version ?? ""));
  const [selectedVersion, setSelectedVersion] = createSignal<string | null>(
    searchParams.version ? String(searchParams.version) : null
  );
  // Fabric API 附加下载（Modloader 步可选，装完实例后自动安装）。
  const [fabricApi, setFabricApi] = createSignal(false);

  // Step 2
  const [loaderType, setLoaderType] = createSignal<string | null>(
    searchParams.loader ? String(searchParams.loader) : null
  );
  const [loaderVersions, setLoaderVersions] = createSignal<LoaderVersion[]>([]);
  const [loaderVersion, setLoaderVersion] = createSignal("");
  /** loader 版本加载代际：快速连点不同 loader 时，晚到的旧结果不得覆盖新结果。 */
  let loaderGen = 0;

  // Step 3
  const [name, setName] = createSignal("");
  const [nameError, setNameError] = createSignal<string | null>(null);

  // Step 4
  const [installing, setInstalling] = createSignal(false);

  const TYPES: Record<string, ReleaseType[]> = {
    release: ["release"],
    snapshot: ["snapshot"],
    old: ["old_alpha", "old_beta"],
  };

  onMount(async () => {
    // 先回缓存（秒开；启动时 initStores 已刷新过一次 manifest），再对齐最新值。
    setVersions_(versionCache.gameVersionsWithRefresh());
    void versionCache.refreshGameVersions().then((list) => {
      if (list) setVersions_(list);
    });
    const local = await instances.list();
    if (local.ok) setLocalVersions(local.data);
  });

  // Error surfaces only when there is nothing to render — a stale cache keeps
  // the list usable while the background refresh retries.
  const error = () => ((versions_() ?? []).length > 0 ? null : versionCache.gameVersionsError());
  const retryVersions = async (): Promise<void> => {
    const list = await versionCache.refreshGameVersions();
    if (list) setVersions_(list);
  };

  const filtered = createMemo(() => {
    const list = versions_() ?? [];
    const query = search().trim().toLowerCase();
    const allowed = TYPES[typeFilter()];
    return list.filter((v) => {
      if (typeFilter() !== "all" && !allowed.includes(v.release_type)) return false;
      if (query && !v.id.toLowerCase().includes(query)) return false;
      return true;
    });
  });

  // Number of local instances per game version (HMCL shows "installed" once;
  // the count makes duplicates understandable without opening the list).
  const versionInstallCounts = createMemo(() => {
    const counts = new Map<string, number>();
    for (const profile of instancesStore.items() ?? []) {
      const gv = profile.summary.game_version;
      counts.set(gv, (counts.get(gv) ?? 0) + 1);
    }
    return counts;
  });

  // 游戏版本变化时，已选 loader 的版本列表同步重载（缓存 + 代际保护防错乱）。
  createEffect(() => {
    const lt = loaderType();
    const gv = selectedVersion();
    if (lt) void pickLoader(lt, gv ?? "");
  });

  async function pickLoader(type: string, gameVersion?: string): Promise<void> {
    setLoaderType(type);
    const gen = ++loaderGen;
    const gv = gameVersion ?? selectedVersion() ?? "";
    // 先回缓存秒显，再后台刷新（用户打开 Modloader 页即触发一次更新）。
    const cached = versionCache.cachedLoaderVersions(type, gv);
    if (cached) applyLoaderVersions(cached);
    const fresh = await versionCache.refreshLoaderVersions(type, gv);
    if (gen !== loaderGen) return; // 已切到其他 loader/版本，丢弃过期结果
    applyLoaderVersions(fresh);
  }

  function applyLoaderVersions(list: LoaderVersion[]): void {
    setLoaderVersions(list);
    setLoaderVersion((current) => {
      // 保留现有选择版本号（仍存在时），否则回落到最新稳定版。
      if (current && list.some((v) => v.version === current)) return current;
      return list.find((v) => v.stable)?.version ?? list[0]?.version ?? "";
    });
  }

  function validateName(): boolean {
    const value = name().trim();
    if (!value) {
      setNameError(t("download.wizard.nameRequired"));
      return false;
    }
    if (value.length > 64) {
      setNameError(t("download.wizard.nameTooLong"));
      return false;
    }
    if ((instancesStore.items() ?? []).some((p) => p.summary.display_name === value)) {
      setNameError(t("download.wizard.nameTaken"));
      return false;
    }
    setNameError(null);
    return true;
  }

  async function execute(): Promise<void> {
    if (!selectedVersion()) return;
    setInstalling(true);
    const directory = await gameDirectories.selected();
    if (!directory.ok) {
      setInstalling(false);
      toast({ message: directory.error.message, tone: "error" });
      return;
    }
    const instanceId = name().trim() || selectedVersion()!;
    const res = await instances.install(
      directory.data.id,
      selectedVersion()!,
      instanceId,
      loaderType() ?? undefined,
      loaderVersion() || undefined,
    );
    if (!res.ok) {
      setInstalling(false);
      toast({ message: res.error.message, tone: "error" });
      return;
    }
    toast({ message: t("tasks.download.started", { name: selectedVersion() ?? "" }), tone: "info" });
    uiStore.setTaskCenterOpen(true);
    setInstalling(false);
    setStep(5);
    void instancesStore.refresh(true);
  }

  /** 从 Modrinth 拉取与游戏版本匹配的 Fabric API 并装入实例（附加选项）。 */
  async function installFabricApi(key: string, gameVersion: string): Promise<void> {
    const loader = loaderType();
    if (loader !== "fabric" && loader !== "quilt") {
      toast({ message: t("download.wizard.fabricApi.skipped"), tone: "info" });
      return;
    }
    const search = await mods.search("fabric-api", gameVersion, "fabric", 1, 10);
    const hit = search.ok
      ? (search.data.hits.find((h) => h.slug === "fabric-api") ?? search.data.hits[0])
      : null;
    if (!hit) {
      toast({ message: t("download.wizard.fabricApi.notFound"), tone: "error" });
      return;
    }
    const vers = await mods.getVersions(hit.id);
    const list = vers.ok ? vers.data : [];
    const mv =
      list.find((v) => v.game_versions.includes(gameVersion) && v.loaders.includes("fabric")) ??
      list[0];
    if (!mv) {
      toast({ message: t("download.wizard.fabricApi.notFound"), tone: "error" });
      return;
    }
    const inst = await mods.install(key, mv);
    toast({
      message: inst.ok
        ? t("download.wizard.fabricApi.installed", { version: mv.version_number })
        : inst.error.message,
      tone: inst.ok ? "success" : "error",
    });
  }

  const defaultName = createMemo(() => {
    const loader = loaderType();
    return loader && selectedVersion() ? `${selectedVersion()}-${loader}` : selectedVersion() ?? "";
  });

  return (
    <div class="wizard">
      <div class="wizard-steps">
        <For each={[1, 2, 3, 4]}>
          {(n) => (
            <button class={`wizard-step${step() === n ? " is-active" : ""}${step() > n ? " is-done" : ""}`} onClick={() => n < step() && setStep(n)}>
              {t(`download.wizard.step.${n}` as TranslationKey)}
            </button>
          )}
        </For>
      </div>

      <Switch>
        {/* Step 1: version */}
        <Match when={step() === 1}>
          <div class="wizard-body">
          <div class="toolbar wizard-toolbar">
            <RmInput value={search()} onInput={setSearch} placeholder={t("download.wizard.searchVersion")} />
            <RmSelect
              value={typeFilter()}
              onChange={(v) => setTypeFilter(v as VersionTypeFilter)}
              options={[
                { value: "release", label: t("download.wizard.type.release") },
                { value: "snapshot", label: t("download.wizard.type.snapshot") },
                { value: "old", label: t("download.wizard.type.old") },
                { value: "all", label: t("download.wizard.type.all") },
              ]}
            />
            <RmButton size="sm" icon="refresh" onClick={() => void versionCache.refreshGameVersions().then((l) => l && setVersions_(l))} />
          </div>
          <Show when={error()} fallback={
            <VirtualList
              items={filtered()}
              itemHeight={46}
              height="fill"
              class="version-list"
              renderItem={(version) => (
                <RmListRow
                  icon={<img class="mc-icon" src={versionTypeIcon(version.release_type)} alt="" loading="lazy" />}
                  title={version.id}
                  selected={selectedVersion() === version.id}
                  onClick={() => setSelectedVersion(version.id)}
                  // 双击直接进入下一步（Modloader 选择）
                  onDblClick={() => {
                    setSelectedVersion(version.id);
                    setStep(2);
                  }}
                  trailing={
                    <div class="btn-row version-row-meta">
                      <span class={`version-channel-label is-${version.release_type}`}>
                        <i />
                        {version.release_type === "release"
                          ? t("download.wizard.type.release")
                          : version.release_type === "snapshot"
                            ? t("download.wizard.type.snapshot")
                            : t("download.wizard.type.old")}
                      </span>
                      <span class="row-date">{formatDate(version.release_time)}</span>
                      <Show when={versionInstallCounts().get(version.id)}>
                        {(count) => (
                          <RmTag tone="success">
                            {count() > 1
                              ? t("download.wizard.installedCount", { n: count() })
                              : t("download.wizard.installed")}
                          </RmTag>
                        )}
                      </Show>
                    </div>
                  }
                />
              )}
            />
          }>
            <RmErrorState message={error()!} onRetry={() => void retryVersions()} />
          </Show>
          <div class="wizard-actions">
            <RmButton variant="ghost" onClick={() => setBindLocalOpen(true)}>{t("download.wizard.bindLocal")}</RmButton>
            <RmButton variant="primary" disabled={!selectedVersion()} onClick={() => setStep(2)}>
              {t("common.next")}
            </RmButton>
          </div>
          <RmDialog open={bindLocalOpen()} title={t("download.wizard.bindLocal")} onClose={() => setBindLocalOpen(false)}>
            <div class="rm-list">
              <For each={localVersions()}>
                {(local) => (
                  <RmListRow
                    title={local.summary.display_name}
                    subtitle={`${local.summary.game_version}${local.summary.loader ? ` · ${loaderLabel(local.summary.loader.kind)} ${local.summary.loader.version ?? ""}` : ""}`}
                    trailing={
                      <RmTag tone={local.directory.kind === "external" ? "success" : "neutral"}>
                        {local.directory.kind === "external" ? t("instances.root.external") : t("instances.root.primary")}
                      </RmTag>
                    }
                    onClick={() => {
                      setSelectedVersion(local.summary.id);
                      setBindLocalOpen(false);
                    }}
                  />
                )}
              </For>
            </div>
          </RmDialog>
          </div>
        </Match>

        {/* Step 2: loader */}
        <Match when={step() === 2}>
          <div class="wizard-body">
          <div class="loader-grid">
            <div class={`loader-card${loaderType() === null ? " is-current" : ""}`} onClick={() => setLoaderType(null)}>
              <div class="loader-card-name">{t("download.wizard.loader.skip")}</div>
              <RmTag>{t("download.wizard.loader.vanilla")}</RmTag>
            </div>
            <div
              class={`loader-card${loaderType() === "fabric" ? " is-current" : ""}`}
              onClick={() => void pickLoader("fabric")}
            >
              <div class="loader-card-head">
                <img class="loader-card-icon" src={loaderIcon("fabric") ?? ""} alt="" loading="lazy" />
                <div class="loader-card-name">{loaderLabel("fabric")}</div>
              </div>
              <Show when={loaderType() === "fabric" && loaderVersion()}>
                <RmTag tone="accent">{loaderVersion()}</RmTag>
              </Show>
            </div>
            <div
              class={`loader-card${fabricApi() ? " is-current" : ""}`}
              onClick={() => setFabricApi((v) => !v)}
            >
              <div class="loader-card-head">
                <img class="loader-card-icon" src={loaderIcon("fabric") ?? ""} alt="" loading="lazy" />
                <div class="loader-card-name">Fabric API</div>
              </div>
              <Show when={fabricApi()} fallback={<RmTag>{t("download.wizard.fabricApi.optional")}</RmTag>}>
                <RmTag tone="accent">{t("download.wizard.fabricApi.added")}</RmTag>
              </Show>
            </div>
            <For each={["neo_forge", "forge", "quilt", "opti_fine", "lite_loader"]}>
              {(type) => (
                <div class={`loader-card${loaderType() === type ? " is-current" : ""}`} onClick={() => void pickLoader(type)}>
                  <div class="loader-card-head">
                    <img class="loader-card-icon" src={loaderIcon(type) ?? ""} alt="" loading="lazy" />
                    <div class="loader-card-name">{loaderLabel(type)}</div>
                  </div>
                  <Show when={loaderType() === type && loaderVersion()}>
                    <RmTag tone="accent">{loaderVersion()}</RmTag>
                  </Show>
                </div>
              )}
            </For>
          </div>
          {/* Fabric 提示移到底部操作行（压缩纵向空间），不再单独占一行 */}
          {/* 选中加载器后弹出具体版本列表：左版本号，右状态 */}
          <Show when={loaderType() && loaderVersions().length > 0}>
            <div class="loader-version-pick">
              <VirtualList
                items={loaderVersions()}
                itemHeight={48}
                height="fill"
                class="version-list"
                renderItem={(loader) => (
                  <RmListRow
                    title={loader.version}
                    selected={loaderVersion() === loader.version}
                    onClick={() => setLoaderVersion(loader.version)}
                    trailing={
                      <Show when={loader.stable}>
                        <RmTag tone="success">{t("download.wizard.loaderStable")}</RmTag>
                      </Show>
                    }
                  />
                )}
              />
            </div>
          </Show>
          <div class="wizard-actions">
            <RmButton variant="ghost" onClick={() => setStep(1)}>{t("common.back")}</RmButton>
            <Show when={loaderType() === "fabric" || loaderType() === "quilt"}>
              <div class="wizard-actions-note">{t("download.wizard.loader.apiHint")}</div>
            </Show>
            <RmButton variant="primary" onClick={() => setStep(3)}>{t("common.next")}</RmButton>
          </div>
          </div>
        </Match>

        {/* Step 3: instance info */}
        <Match when={step() === 3}>
          <div class="wizard-form">
            <div class="rm-field">
              <div class="rm-field-label">{t("download.wizard.instanceName")}</div>
              <RmInput
                value={name()}
                onInput={(v) => {
                  setName(v);
                  void validateName();
                }}
                placeholder={defaultName()}
                error={nameError()}
              />
            </div>
            <div class="muted-line">{t("download.wizard.memoryNote")}</div>
          </div>
          <div class="wizard-actions">
            <RmButton variant="ghost" onClick={() => setStep(2)}>{t("common.back")}</RmButton>
            <RmButton
              variant="primary"
              onClick={() => {
                if (!name().trim()) setName(defaultName());
                if (validateName() || (name().trim() && !nameError())) setStep(4);
              }}
            >
              {t("common.next")}
            </RmButton>
          </div>
        </Match>

        {/* Step 4: execute */}
        <Match when={step() === 4}>
          <div class="wizard-execute">
            <div class="wizard-execute-summary">
              <RmTag tone="accent">{selectedVersion()}</RmTag>
              <Show when={loaderType()}>
                <RmTag tone="accent">{loaderLabel(loaderType())} {loaderVersion()}</RmTag>
              </Show>
              <span>{name() || defaultName()}</span>
            </div>
            <Show when={installing()}>
              <RmProgress value={0} determinate={false} />
              <div class="muted-line">{t("download.wizard.backgroundHint")}</div>
            </Show>
          </div>
          <div class="wizard-actions">
            <RmButton variant="ghost" onClick={() => setStep(3)}>{t("common.back")}</RmButton>
            <RmButton variant="primary" icon="download" loading={installing()} onClick={() => void execute()}>
              {t("common.install")}
            </RmButton>
          </div>
        </Match>

        {/* Step 5: done */}
        <Match when={step() === 5}>
          <RmCard title={t("download.wizard.done.title")}>
            <div class="btn-row">
              <RmButton variant="primary" icon="play" onClick={() => navigate(`/instances`)}>
                {t("download.wizard.done.view")}
              </RmButton>
              <RmButton icon="package" onClick={() => navigate("/download#mods")}>
                {t("download.wizard.done.mods")}
              </RmButton>
            </div>
          </RmCard>
        </Match>
      </Switch>
    </div>
  );
}

// ── Mod store ──

function ModStore(): JSX.Element {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  // Reactive so the ⚓ exit chip (navigate without ?instance=) clears the
  // context without remounting the page.
  const contextProfile = createMemo(() => {
    const id = searchParams.instance ? String(searchParams.instance) : null;
    return id ? (instancesStore.items() ?? []).find((p) => p.summary.key === id) ?? null : null;
  });

  const [source, setSource] = createSignal<"modrinth" | "curseforge">("modrinth");
  const [query, setQuery] = createSignal("");
  const [gameVersion, setGameVersion] = createSignal(contextProfile()?.summary.game_version ?? "");
  const [loader, setLoader] = createSignal(
    contextProfile()?.summary.loader ? String(contextProfile()!.summary.loader!.kind) : ""
  );
  const [sort, setSort] = createSignal<ModSort>("downloads");
  const [result, setResult] = createSignal<ModSearchResult | null>(null);
  const [page, setPage] = createSignal(1);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const [detail, setDetail] = createSignal<ModInfo | null>(null);
  const [detailVersions, setDetailVersions] = createSignal<ModVersion[]>([]);
  const [detailLoading, setDetailLoading] = createSignal(false);
  const [detailError, setDetailError] = createSignal<string | null>(null);
  const [installedFiles, setInstalledFiles] = createSignal<Set<string>>(new Set());
  const [installDialog, setInstallDialog] = createSignal<{ version: ModVersion } | null>(null);

  // Game-version filter options come from the launcher manifest (HMCL-style
  // "world.game_version" picker), not just the handful of installed instances.
  const [manifest, setManifest] = createSignal<VersionInfo[]>(versionCache.cachedGameVersions() ?? []);
  onMount(() => {
    const cached = versionCache.cachedGameVersions();
    if (cached) setManifest(cached);
    else void versionCache.refreshGameVersions().then((list) => list && setManifest(list));
  });

  const gameVersionOptions = createMemo(() => {
    const releases = manifest()
      .filter((v) => v.release_type === "release")
      .map((v) => v.id);
    const installed = (instancesStore.items() ?? []).map((p) => p.summary.game_version);
    const values = sortGameVersionsDesc([...releases, ...installed, gameVersion()].filter(Boolean));
    return [
      { value: "", label: t("modStore.filter.anyVersion") },
      ...values.map((v) => ({ value: v, label: v })),
    ];
  });

  const loaderOptions = createMemo(() => {
    const known = MOD_LOADER_FILTERS.map((l) => ({ value: l.id, label: l.label }));
    if (loader() && !known.some((l) => l.value === loader())) {
      known.push({ value: loader(), label: loaderLabel(loader()) });
    }
    return [{ value: "", label: t("modStore.filter.anyLoader") }, ...known];
  });

  let searchSeq = 0;

  async function runSearch(targetPage = 1): Promise<void> {
    const seq = ++searchSeq;
    setLoading(true);
    const normalized = normalizeLoaderId(loader());
    // Modrinth facets use `neoforge` / `liteloader`; the launcher's internal
    // ids use `neo_forge` / `lite_loader`.
    const facetLoader = !normalized
      ? undefined
      : normalized === "neo_forge"
        ? "neoforge"
        : normalized === "lite_loader"
          ? "liteloader"
          : normalized;
    const res = await mods.search(
      query().trim(),
      gameVersion() || undefined,
      facetLoader,
      targetPage,
      20,
      sort()
    );
    if (seq !== searchSeq) return; // stale response
    setLoading(false);
    if (res.ok) {
      setResult(res.data);
      setPage(targetPage);
      setError(null);
      // stale-while-revalidate：先显示缓存图，后台条件刷新本页可见图标。
      for (const hit of res.data.hits) {
        if (hit.icon_url) cacheStore.refreshIcon(hit.icon_url, "modicon");
      }
    } else {
      setError(res.error.message);
    }
  }

  createEffect(() => {
    // Auto-search on filter changes (300ms debounce); the explicit button and
    // Enter both call the same path.
    void source();
    void query();
    void gameVersion();
    void loader();
    void sort();
    const id = setTimeout(() => void runSearch(1), 300);
    return () => clearTimeout(id);
  });

  // The instance list may finish loading after ModStore is created; apply the
  // context's locked filters once the profile becomes available (and when the
  // user jumps from one instance to another).
  let appliedContextKey: string | null = null;
  createEffect(() => {
    const profile = contextProfile();
    if (!profile) {
      appliedContextKey = null;
      return;
    }
    if (profile.summary.key === appliedContextKey) return;
    appliedContextKey = profile.summary.key;
    setGameVersion(profile.summary.game_version);
    const loaderKind = profile.summary.loader ? normalizeLoaderId(String(profile.summary.loader.kind)) : "";
    // OptiFine etc. are not Modrinth loader facets — keep the search global
    // rather than silently returning zero results for an unsupported facet.
    setLoader(MOD_LOADER_FILTERS.some((l) => l.id === loaderKind) ? loaderKind : "");
  });

  let detailSeq = 0;

  async function openDetail(mod: ModInfo): Promise<void> {
    const seq = ++detailSeq;
    setDetail(mod);
    // Never briefly render the previous mod's versions under the new header.
    setDetailVersions([]);
    setDetailError(null);
    setDetailLoading(true);
    setInstalledFiles(new Set<string>());
    const profile = contextProfile();
    const [res, listRes] = await Promise.all([
      mods.getVersions(mod.id),
      profile ? mods.list(profile.summary.key) : Promise.resolve(null),
    ]);
    if (seq !== detailSeq) return;
    setDetailLoading(false);
    if (res.ok) setDetailVersions(res.data);
    else setDetailError(res.error.message);
    if (listRes?.ok) {
      setInstalledFiles(new Set<string>([...listRes.data.enabled, ...listRes.data.disabled]));
    }
  }

  function closeDetail(): void {
    detailSeq += 1;
    setDetail(null);
    setDetailVersions([]);
    setDetailError(null);
    setInstalledFiles(new Set<string>());
  }

  const recommended = createMemo(() => {
    const profile = contextProfile();
    if (!profile) return null;
    const targetLoader = profile.summary.loader
      ? normalizeLoaderId(String(profile.summary.loader.kind))
      : null;
    return (
      detailVersions().find(
        (v) =>
          v.game_versions.includes(profile.summary.game_version) &&
          (!targetLoader || v.loaders.some((l) => normalizeLoaderId(l) === targetLoader))
      ) ?? null
    );
  });

  async function installVersion(version: ModVersion, profile: InstanceDetail): Promise<void> {
    // 即时反馈：后端创建统一任务，事件把这一行接入任务中心。
    setInstallDialog(null);
    toast({ message: t("tasks.download.started", { name: version.filename ?? version.version_number }), tone: "info" });
    const res = await mods.install(profile.summary.key, version);
    if (res.ok) {
      // Only the instance being browsed owns the "installed" markers.
      if (version.filename && contextProfile()?.summary.key === profile.summary.key) {
        setInstalledFiles((prev) => {
          const next = new Set(prev);
          next.add(version.filename!);
          return next;
        });
      }
      toast({
        message: t("modStore.installedTo", { file: res.data, name: profile.summary.display_name }),
        tone: "success",
        actionLabel: t("modStore.viewInstance"),
        onAction: () => navigate(`/instances/${profile.summary.key}`),
      });
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function copyLink(version: ModVersion): Promise<void> {
    const res = await mods.getDownloadUrl(version.id);
    if (res.ok) {
      void navigator.clipboard.writeText(res.data);
      toast({ message: t("common.copied"), tone: "success" });
    }
  }

  function openProject(mod: ModInfo): void {
    if (mod.source === "modrinth") void external.openUrl(`https://modrinth.com/mod/${mod.slug}`);
  }

  const totalPages = () => Math.max(1, Math.ceil((result()?.total ?? 0) / 20));

  return (
    <div class="mod-store">
      <Show when={detail() === null} fallback={
        <ModDetail
          mod={detail()!}
          versions={detailVersions()}
          loading={detailLoading()}
          error={detailError()}
          installedFiles={installedFiles()}
          recommended={recommended()}
          profile={contextProfile()}
          onBack={closeDetail}
          onRetry={() => void openDetail(detail()!)}
          onInstall={(version) => {
            const profile = contextProfile();
            if (profile) void installVersion(version, profile);
            else setInstallDialog({ version });
          }}
          onCopyLink={(version) => void copyLink(version)}
          onOpenBrowser={(version) => {
            if (version.download_url) void external.openUrl(version.download_url);
          }}
          onOpenProject={() => openProject(detail()!)}
        />
      }>
        <Show when={contextProfile()}>
          {(profile) => (
            <div class="store-context">
              <span class="store-context-pin">⚓</span>
              <span class="store-context-copy">
                {t("modStore.contextBar", { name: profile().summary.display_name })}
              </span>
              <span class="store-context-filter">
                {profile().summary.game_version}
                {profile().summary.loader ? ` · ${loaderLabel(String(profile().summary.loader!.kind))}` : ""}
              </span>
              <button
                class="store-context-exit"
                onClick={() => {
                  navigate("/download#mods");
                  setGameVersion("");
                  setLoader("");
                }}
              >
                <Icon name="x" size={12} />
                {t("modStore.exitContext")}
              </button>
            </div>
          )}
        </Show>

        <section class="store-filter-card">
          <div class="store-filter-meta">
            {/* 下载源切换（HMCL 式 CF / MO；CurseForge 源尚未接入后端） */}
            <div class="source-switch store-source" role="tablist">
              <button
                class={`source-chip${source() === "modrinth" ? " is-active" : ""}`}
                onClick={() => {
                  if (source() !== "modrinth") {
                    setSource("modrinth");
                    void runSearch(1);
                  }
                }}
              >
                {t("modStore.source.modrinth")}
              </button>
              <button
                class="source-chip"
                disabled
                title={t("modStore.source.unavailable")}
                onClick={() => toast({ message: t("modStore.source.unavailable"), tone: "info" })}
              >
                {t("modStore.source.curseforge")}
                <span class="source-chip-note">{t("modStore.source.comingSoon")}</span>
              </button>
            </div>

            <span class="store-result-count">
              {result() ? t("modStore.resultCount", { total: result()!.total }) : ""}
            </span>
            <Show when={loading()}>
              <span class="store-loading"><i />{t("modStore.loading")}</span>
            </Show>

            <div class="store-pager">
              <RmIconButton
                icon="chevron-left"
                label={t("common.prevPage")}
                disabled={page() <= 1}
                onClick={() => void runSearch(page() - 1)}
              />
              <span>{t("modStore.page", { page: page(), total: totalPages() })}</span>
              <RmIconButton
                icon="chevron-right"
                label={t("common.nextPage")}
                disabled={page() >= totalPages()}
                onClick={() => void runSearch(page() + 1)}
              />
            </div>
          </div>

          <div class="store-filter-main">
            <RmInput
              value={query()}
              onInput={setQuery}
              placeholder={t("modStore.search.placeholder")}
              onEnter={() => void runSearch(1)}
            />
            <RmSelect
              class="store-select store-version-select"
              value={gameVersion()}
              disabled={!!contextProfile()}
              onChange={setGameVersion}
              options={gameVersionOptions()}
            />
            <RmSelect
              class="store-select store-loader-select"
              value={loader()}
              disabled={!!contextProfile()}
              onChange={setLoader}
              options={loaderOptions()}
            />
            <RmSelect
              class="store-select store-sort-select"
              value={sort()}
              onChange={(v) => setSort(v as ModSort)}
              options={MOD_SORTS.map((s) => ({ value: s.value, label: t(s.label) }))}
            />
            <RmButton size="sm" variant="primary" icon="search" onClick={() => void runSearch(1)}>
              {t("modStore.search.button")}
            </RmButton>
          </div>
        </section>

        <div class="mod-result-list">
          <Switch>
            <Match when={error()}>
              <RmErrorState message={error() ?? ""} onRetry={() => void runSearch(page())} />
            </Match>
            <Match when={!result() && loading()}>
              <RmSkeleton rows={6} />
            </Match>
            <Match when={result()}>
              <For each={result()!.hits}>
                {(mod) => (
                  <ModResultRow
                    mod={mod}
                    highlightVersion={gameVersion() || undefined}
                    onOpen={() => void openDetail(mod)}
                  />
                )}
              </For>
              <Show when={result()!.hits.length === 0 && !loading()}>
                <RmEmptyState icon="search" title={t("modStore.empty")} />
              </Show>
            </Match>
          </Switch>
        </div>

        {/* No-context install: pick a target profile */}
        <RmDialog
          open={installDialog() !== null}
          title={t("modStore.installTo.title")}
          onClose={() => setInstallDialog(null)}
        >
          <div class="rm-list">
            <For each={instancesStore.items() ?? []}>
              {(profile) => {
                const version = installDialog()?.version;
                if (!version) return null;
                const compatible =
                  version.loaders.length === 0 ||
                  (profile.summary.loader
                    ? version.loaders.some(
                        (l) => normalizeLoaderId(l) === normalizeLoaderId(String(profile.summary.loader!.kind))
                      )
                    : version.game_versions.includes(profile.summary.game_version));
                return (
                  <RmListRow
                    title={profile.summary.display_name}
                    subtitle={profile.summary.game_version}
                    onClick={() => void installVersion(version, profile)}
                    trailing={
                      <Show when={!compatible}>
                        <RmTag tone="warning">{t("modStore.compatibleWarning")}</RmTag>
                      </Show>
                    }
                  />
                );
              }}
            </For>
          </div>
        </RmDialog>
      </Show>
    </div>
  );
}

/** HMCL-style compact result row: name + description, compatibility, downloads. */
function ModResultRow(props: { mod: ModInfo; highlightVersion?: string; onOpen: () => void }): JSX.Element {
  const versions = createMemo(() => {
    const list = sortGameVersionsDesc(props.mod.game_versions ?? []).filter(Boolean);
    if (!props.highlightVersion || !list.includes(props.highlightVersion)) return list;
    return [props.highlightVersion, ...list.filter((v) => v !== props.highlightVersion)];
  });
  const loaders = createMemo(() => [
    ...new Set((props.mod.loaders ?? []).map(normalizeLoaderId).filter(Boolean)),
  ]);
  const tags = createMemo(() => {
    if (loaders().length > 0) return loaders().map((id) => ({ id, label: loaderLabel(id) }));
    return (props.mod.categories ?? [])
      .filter((c) => c !== "minecraft")
      .slice(0, 2)
      .map((id) => ({ id, label: id }));
  });
  const moreVersions = () => Math.max(0, versions().length - 2);

  return (
    <button
      class="mod-result"
      type="button"
      onClick={props.onOpen}
      title={versions().length > 0 ? t("modStore.supportedVersions", { versions: versions().join(", ") }) : props.mod.name}
    >
      <span class="mod-icon">
        <ModIcon url={props.mod.icon_url} name={props.mod.name} />
      </span>
      <span class="mod-result-main">
        <span class="mod-result-head">
          <span class="mod-result-name">{props.mod.name}</span>
          <span class="mod-result-author">{props.mod.authors.join(", ")}</span>
        </span>
        <span class="mod-result-desc">{props.mod.description}</span>
      </span>
      <span class="mod-result-tags">
        <For each={tags()}>{(tag) => <span class="mod-loader-chip">{tag.label}</span>}</For>
      </span>
      <span class="mod-result-versions">
        <Show when={versions().length > 0} fallback={<span class="mod-version-more">—</span>}>
          <span class="mod-version-compat-label">{t("modStore.compat")}</span>
          <For each={versions().slice(0, 2)}>{(v) => <span class="mod-version-chip">{v}</span>}</For>
          <Show when={moreVersions() > 0}>
            <span class="mod-version-more" title={versions().join(", ")}>
              {t("modStore.moreVersions", { n: moreVersions() })}
            </span>
          </Show>
        </Show>
      </span>
      <span class="mod-result-downloads" title={String(props.mod.download_count)}>
        <Icon name="download" size={12} />
        {abbreviateCount(props.mod.download_count)}
      </span>
      <Icon name="chevron-right" size={16} class="mod-result-chevron" />
    </button>
  );
}

function ModDetail(props: {
  mod: ModInfo;
  versions: ModVersion[];
  loading: boolean;
  error: string | null;
  installedFiles: Set<string>;
  recommended: ModVersion | null;
  profile: InstanceDetail | null;
  onBack: () => void;
  onRetry: () => void;
  onInstall: (version: ModVersion) => void;
  onCopyLink: (version: ModVersion) => void;
  onOpenBrowser: (version: ModVersion) => void;
  onOpenProject: () => void;
}): JSX.Element {
  const [gameFilter, setGameFilter] = createSignal("");
  const [loaderFilter, setLoaderFilter] = createSignal("");
  const [channelFilter, setChannelFilter] = createSignal<VersionChannel | "all">("all");
  // Groups default to open for the newest two; explicit toggles are keyed by
  // group so filtering does not preserve a stale open/closed state.
  const [groupOpen, setGroupOpen] = createSignal<Record<string, boolean>>({});

  const gameVersions = createMemo(() =>
    sortGameVersionsDesc(props.versions.flatMap((v) => v.game_versions).filter(Boolean))
  );
  const loaders = createMemo(() => {
    const ids = [...new Set(props.versions.flatMap((v) => v.loaders).map(normalizeLoaderId).filter(Boolean))];
    return ids.sort((a, b) => loaderLabel(a).localeCompare(loaderLabel(b)));
  });
  const filtered = createMemo(() =>
    filterModVersions(props.versions, {
      gameVersion: gameFilter() || undefined,
      loader: loaderFilter() || undefined,
      channel: channelFilter(),
    })
  );
  const groups = createMemo(() => groupModVersions(filtered(), gameFilter() || undefined));

  function setGame(value: string): void {
    setGameFilter(value);
    setGroupOpen({});
  }
  function setLoaderFilterValue(value: string): void {
    setLoaderFilter(value);
    setGroupOpen({});
  }
  function setChannel(value: VersionChannel | "all"): void {
    setChannelFilter(value);
    setGroupOpen({});
  }
  function isOpen(key: string, index: number): boolean {
    return groupOpen()[key] ?? index < 2;
  }
  function toggleGroup(key: string, index: number): void {
    setGroupOpen({ ...groupOpen(), [key]: !isOpen(key, index) });
  }

  return (
    <div class="mod-detail">
      <button class="mod-detail-back" onClick={props.onBack}>
        <Icon name="chevron-left" size={14} />
        {t("modStore.backToResults")}
      </button>

      <section class="mod-detail-hero">
        <div class="mod-icon is-large">
          <ModIcon url={props.mod.icon_url} name={props.mod.name} />
        </div>
        <div class="mod-detail-copy">
          <div class="mod-detail-title-row">
            <h2>{props.mod.name}</h2>
            <Show when={props.profile}>
              {(profile) => (
                <span class="mod-context-pill">
                  ⚓ {profile().summary.display_name}
                </span>
              )}
            </Show>
          </div>
          <div class="mod-detail-meta">
            <span>{props.mod.authors.join(", ")}</span>
            <span class="mod-detail-dot">·</span>
            <span>{abbreviateCount(props.mod.download_count)} {t("common.downloads")}</span>
            <For each={props.mod.categories.filter((c) => c !== "minecraft").slice(0, 4)}>
              {(category) => <RmTag>{category}</RmTag>}
            </For>
          </div>
          <p class="mod-description">{props.mod.description}</p>
        </div>
        <RmButton size="sm" variant="ghost" icon="external" onClick={props.onOpenProject}>
          {t("modStore.openProject")}
        </RmButton>
      </section>

      <Show when={props.error}>
        <RmErrorState message={props.error ?? ""} onRetry={props.onRetry} />
      </Show>

      <Show when={!props.error && props.loading}>
        <RmSkeleton rows={4} />
      </Show>

      <Show when={!props.error && !props.loading}>
        <Show when={props.recommended} keyed>
          {(version) => (
            <div class="recommended-block">
              <div class="recommended-title">
                <span>⚓ {t("modStore.recommendedForVersion", { version: props.profile?.summary.game_version ?? "" })}</span>
                <Show when={props.profile}>
                  {(profile) => <span class="recommended-target">{profile().summary.display_name}</span>}
                </Show>
              </div>
              <VersionRow
                version={version}
                groupGameVersion={props.profile?.summary.game_version}
                installed={!!version.filename && props.installedFiles.has(version.filename)}
                recommended
                onInstall={() => props.onInstall(version)}
                onCopyLink={() => props.onCopyLink(version)}
                onOpenBrowser={() => props.onOpenBrowser(version)}
              />
            </div>
          )}
        </Show>

        <div class="mod-version-toolbar">
          <div class="mod-version-toolbar-title">
            <strong>{t("modStore.versions")}</strong>
            <span>{t("modStore.versionCount", { n: props.versions.length })}</span>
          </div>
          <div class="mod-version-filters">
            <RmSelect
              class="mod-version-filter mod-version-filter-game"
              value={gameFilter()}
              onChange={setGame}
              options={[
                { value: "", label: t("modStore.filter.allVersions") },
                ...gameVersions().map((v) => ({
                  value: v,
                  label: isSnapshotGameVersion(v) ? t("modStore.group.snapshot", { version: v }) : v,
                })),
              ]}
            />
            <RmSelect
              class="mod-version-filter mod-version-filter-loader"
              value={loaderFilter()}
              onChange={setLoaderFilterValue}
              options={[
                { value: "", label: t("modStore.filter.allLoaders") },
                ...loaders().map((id) => ({ value: id, label: loaderLabel(id) })),
              ]}
            />
            <div class="channel-tabs" role="tablist" aria-label={t("modStore.filter.channel")}>
              <For each={["all", "release", "beta", "alpha"] as const}>
                {(value) => (
                  <button
                    class={`rm-chip${channelFilter() === value ? " is-active" : ""}`}
                    role="tab"
                    aria-selected={channelFilter() === value}
                    onClick={() => setChannel(value)}
                  >
                    {value === "all" ? t("modStore.filter.allChannels") : channelLabel(value)}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        <Show
          when={groups().length > 0}
          fallback={
            <RmEmptyState
              icon="search"
              title={t("modStore.noVersions")}
              hint={t("modStore.noVersionsHint")}
            />
          }
        >
          <div class="version-groups">
            <For each={groups()}>
              {(group, index) => (
                <section class="version-group">
                  <button
                    class="version-group-head"
                    aria-expanded={isOpen(group.key, index())}
                    onClick={() => toggleGroup(group.key, index())}
                  >
                    <Icon
                      name={isOpen(group.key, index()) ? "chevron-down" : "chevron-right"}
                      size={14}
                    />
                    <span class="version-group-title">
                      {group.gameVersion === ""
                        ? t("modStore.group.unknown")
                        : group.snapshot
                          ? t("modStore.group.snapshot", { version: group.gameVersion })
                          : `Minecraft ${group.gameVersion}`}
                    </span>
                    <span class="version-group-count">
                      {t("modStore.versionCount", { n: group.versions.length })}
                    </span>
                    <Show when={group.latest.published}>
                      <span class="version-group-date">{formatDate(group.latest.published)}</span>
                    </Show>
                  </button>
                  <Show when={isOpen(group.key, index())}>
                    <div class="version-group-body">
                      <For each={group.versions}>
                        {(version) => (
                          <VersionRow
                            version={version}
                            groupGameVersion={group.gameVersion}
                            installed={!!version.filename && props.installedFiles.has(version.filename)}
                            recommended={props.recommended?.id === version.id}
                            onInstall={() => props.onInstall(version)}
                            onCopyLink={() => props.onCopyLink(version)}
                            onOpenBrowser={() => props.onOpenBrowser(version)}
                          />
                        )}
                      </For>
                    </div>
                  </Show>
                </section>
              )}
            </For>
          </div>
        </Show>

        <div class="muted-line">{t("modStore.dependencyHint")}</div>
      </Show>
    </div>
  );
}

function VersionRow(props: {
  version: ModVersion;
  groupGameVersion?: string;
  installed?: boolean;
  recommended?: boolean;
  onInstall: () => void;
  onCopyLink: () => void;
  onOpenBrowser: () => void;
}): JSX.Element {
  const channel = () => versionChannel(props.version);
  const title = () => props.version.name?.trim() || props.version.version_number;
  const showNumber = () => title() !== props.version.version_number;
  const versionLoaders = createMemo(() => [
    ...new Set(props.version.loaders.map(normalizeLoaderId).filter(Boolean)),
  ]);
  const extraGameVersions = createMemo(() =>
    sortGameVersionsDesc(props.version.game_versions).filter(
      (v) => v !== (props.groupGameVersion ?? "")
    )
  );
  return (
    <div class={`mod-version-row${props.recommended ? " is-recommended" : ""}`}>
      <span class={`${channelTagClass(channel())}`} title={channelLabel(channel())}>
        <i />
        {channelLabel(channel())}
      </span>
      <span class="mod-version-main">
        <span class="mod-version-title">
          <strong>{title()}</strong>
          <Show when={showNumber()}>
            <span class="mod-version-number">{props.version.version_number}</span>
          </Show>
          <Show when={props.installed}>
            <span class="mod-installed-mark" title={t("modStore.installed")}>✓ {t("modStore.installed")}</span>
          </Show>
          <Show when={props.recommended}>
            <span class="mod-recommended-badge">{t("modStore.recommendedShort")}</span>
          </Show>
        </span>
        <span class="mod-version-meta">
          <Show when={props.version.published}>
            <span>{formatDate(props.version.published)}</span>
          </Show>
          <Show when={props.version.file_size}>
            {(size) => <span>{formatBytes(size())}</span>}
          </Show>
          <For each={versionLoaders()}>
            {(loader) => <span class="mod-loader-chip">{loaderLabel(loader)}</span>}
          </For>
        </span>
      </span>
      <Show when={extraGameVersions().length > 0}>
        <span
          class="mod-version-compat"
          title={t("modStore.supportedVersions", { versions: props.version.game_versions.join(", ") })}
        >
          <span class="mod-version-compat-label">{t("modStore.compat")}</span>
          <For each={extraGameVersions().slice(0, 2)}>
            {(v) => <span class="mod-version-chip">{v}</span>}
          </For>
          <Show when={extraGameVersions().length > 2}>
            <span class="mod-version-more">
              {t("modStore.moreVersions", { n: extraGameVersions().length - 2 })}
            </span>
          </Show>
        </span>
      </Show>
      <span class="mod-version-actions">
        <RmButton
          size="sm"
          variant={props.recommended ? "primary" : "secondary"}
          onClick={props.onInstall}
        >
          {t("modStore.versionInstall")}
        </RmButton>
        <RmIconButton icon="copy" label={t("modStore.copyLink")} onClick={props.onCopyLink} />
        <RmIconButton icon="external" label={t("modStore.openInBrowser")} onClick={props.onOpenBrowser} />
      </span>
    </div>
  );
}


// ── 图标缓存组件与缓存管理 ──

/** 缓存协议优先、直连回退、首字母兜底的 Mod 图标。 */
function ModIcon(props: { url: string | null; name: string }): JSX.Element {
  const [stage, setStage] = createSignal<"cache" | "direct" | "none">("cache");
  createEffect(() => {
    void props.url;
    setStage("cache");
  });
  const src = () => {
    if (!props.url || stage() === "none") return null;
    return stage() === "cache" ? cacheStore.iconSrc(props.url) : props.url;
  };
  return (
    <Show when={src()} fallback={<span>{initialOf(props.name)}</span>}>
      {(url) => (
        <img
          src={url()}
          alt=""
          loading="lazy"
          onError={() => setStage(stage() === "cache" ? "direct" : "none")}
        />
      )}
    </Show>
  );
}
