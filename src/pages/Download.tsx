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
import { RmSideTabs, RmListRow, RmEmptyState, RmErrorState, RmCard, RmFieldRow } from "../components/rm/surfaces";
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
import { downloadsStore } from "../stores/downloads";
import { uiStore, toast } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import { abbreviateCount, formatDate, initialOf } from "../lib/format";
import { versionTypeIcon, loaderIcon } from "../lib/mcIcons";
import { LOADER_CARDS, loaderLabel } from "../lib/loaders";
import { useHashTab } from "../lib/useHashTab";

const TABS: { id: string; icon: "download" | "package" | "layers"; label: TranslationKey }[] = [
  { id: "install", icon: "download", label: "download.tab.install" },
  { id: "mods", icon: "package" as const, label: "download.tab.mods" },
  { id: "modpack", icon: "layers" as const, label: "download.tab.modpack" },
];

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
              <RmEmptyState
                icon="layers"
                title={t("download.modpack.title")}
                hint={t("download.modpack.hint")}
              />
            </Match>
          </Switch>
        </div>
      </div>
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

  const knownVersionIds = createMemo(
    () => new Set((instancesStore.items() ?? []).map((p) => p.summary.game_version))
  );

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
    const key = `${directory.data.id}/${instanceId}`;
    const batchId = `wizard-${key}`;
    downloadsStore.beginInstall({
      batchId,
      profileId: key,
      version: selectedVersion()!,
      label: `${selectedVersion()} · ${instanceId}`,
    });
    const res = await instances.install(
      directory.data.id,
      selectedVersion()!,
      instanceId,
      loaderType() ?? undefined,
      loaderVersion() || undefined,
    );
    if (!res.ok) {
      downloadsStore.finishBatch(batchId, false, res.error.message);
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
              itemHeight={48}
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
                    <div class="btn-row">
                      <span class="row-date">{formatDate(version.release_time)}</span>
                      <Show when={knownVersionIds().has(version.id)}>
                        <RmTag tone="success">{t("download.wizard.installed")}</RmTag>
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
  const [result, setResult] = createSignal<ModSearchResult | null>(null);
  const [page, setPage] = createSignal(1);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [detail, setDetail] = createSignal<ModInfo | null>(null);
  const [detailVersions, setDetailVersions] = createSignal<ModVersion[]>([]);
  const [installDialog, setInstallDialog] = createSignal<{ version: ModVersion } | null>(null);

  let searchSeq = 0;

  async function runSearch(targetPage = 1): Promise<void> {
    const seq = ++searchSeq;
    setLoading(true);
    // 默认（无关键词）按下载量排名；有关键词走相关性排序。
    const ranking = query().trim() === "" ? "downloads" : "relevance";
    const res = await mods.search(
      query().trim(),
      gameVersion() || undefined,
      loader() || undefined,
      targetPage,
      20,
      ranking
    );
    setLoading(false);
    if (seq !== searchSeq) return; // stale response
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
    // Auto-search on filter changes (300ms debounce, ui-design 05 §3.1).
    const id = setTimeout(() => void runSearch(1), 300);
    return () => clearTimeout(id);
  });

  async function openDetail(mod: ModInfo): Promise<void> {
    setDetail(mod);
    const res = await mods.getVersions(mod.id);
    if (res.ok) setDetailVersions(res.data);
    else toast({ message: res.error.message, tone: "error" });
  }

  const recommended = createMemo(() => {
    const profile = contextProfile();
    if (!profile) return null;
    return (
      detailVersions().find(
        (v) =>
          v.game_versions.includes(profile.summary.game_version) &&
          (profile.summary.loader
            ? v.loaders.some((l) => l.toLowerCase() === String(profile.summary.loader!.kind).toLowerCase())
            : true)
      ) ?? null
    );
  });

  async function installVersion(version: ModVersion, profile: InstanceDetail): Promise<void> {
    // 即时反馈（用户要求）：点击即弹提示 + 任务中心入列（合成任务，完成转结果）
    setInstallDialog(null);
    toast({ message: t("tasks.download.started", { name: version.filename ?? version.version_number }), tone: "info" });
    const taskKey = downloadsStore.beginTask(
      t("tasks.modInstall.label", { mod: version.filename ?? version.version_number, instance: profile.summary.display_name })
    );
    const res = await mods.install(profile.summary.key, version);
    if (res.ok) {
      downloadsStore.finishTask(taskKey, true, profile.summary.display_name);
      toast({
        message: t("modStore.installedTo", { file: res.data, name: profile.summary.display_name }),
        tone: "success",
        actionLabel: t("modStore.viewInstance"),
        onAction: () => navigate(`/instances/${profile.summary.key}`),
      });
    } else {
      downloadsStore.finishTask(taskKey, false, res.error.message);
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

  return (
    <div class="mod-store">
      <Show when={contextProfile()}>
        {(profile) => (
          <div class="context-bar">
            ⚓ {t("modStore.contextBar", { name: profile().summary.display_name })}
            <button
              class="rm-chip"
              onClick={() => {
                navigate("/download#mods");
                setGameVersion("");
                setLoader("");
              }}
            >
              {t("modStore.exitContext")}
            </button>
          </div>
        )}
      </Show>

      {/* 下载源切换（HMCL 式 CF / MO；CurseForge 源尚未接入后端） */}
      <div class="source-switch" role="tablist">
        <button
          class={`source-chip${source() === "modrinth" ? " is-active" : ""}`}
          onClick={() => {
            if (source() !== "modrinth") {
              setSource("modrinth");
              void runSearch(1);
            }
          }}
        >
          Modrinth · MO
        </button>
        <button
          class="source-chip"
          disabled
          title={t("modStore.source.unavailable")}
          onClick={() => toast({ message: t("modStore.source.unavailable"), tone: "info" })}
        >
          CurseForge · CF
        </button>
        <span class="muted-line source-ranking-note">
          {query().trim() === "" ? t("modStore.source.rankingNote") : ""}
        </span>
      </div>

      <div class="toolbar">
        <RmInput value={query()} onInput={setQuery} placeholder={t("modStore.search.placeholder")} onEnter={() => void runSearch(1)} />
        <RmSelect
          value={gameVersion()}
          disabled={!!contextProfile()}
          onChange={setGameVersion}
          options={[
            { value: "", label: t("modStore.filter.anyVersion") },
            ...[...new Set((instancesStore.items() ?? []).map((p) => p.summary.game_version))].map(
              (v) => ({ value: v, label: v })
            ),
          ]}
        />
        <RmSelect
          value={loader()}
          disabled={!!contextProfile()}
          onChange={setLoader}
          options={[
            { value: "", label: t("modStore.filter.anyLoader") },
            // Same source as the wizard grid (P2-21): 6 loaders, not 3.
            ...LOADER_CARDS.map((l) => ({ value: l.id, label: l.label })),
          ]}
        />
        <RmButton size="sm" variant="primary" icon="search" onClick={() => void runSearch(1)}>
          {t("modStore.search.button")}
        </RmButton>
      </div>

      <Show
        when={detail() === null}
        fallback={
          <ModDetail
            mod={detail()!}
            versions={detailVersions()}
            recommended={recommended()}
            onBack={() => setDetail(null)}
            onInstall={(version) => {
              const profile = contextProfile();
              if (profile) void installVersion(version, profile);
              else setInstallDialog({ version });
            }}
            onCopyLink={(version) => void copyLink(version)}
            onOpenBrowser={(version) => {
              if (version.download_url) void external.openUrl(version.download_url);
            }}
          />
        }
      >
        <Show when={error()} fallback={<div class="mod-results">
          <For each={result()?.hits ?? []}>
            {(mod) => (
              <RmListRow
                icon={
                  <div class="mod-icon">
                    <ModIcon url={mod.icon_url} name={mod.name} />
                  </div>
                }
                title={mod.name}
                subtitle={`${mod.authors.join(", ")} · ${abbreviateCount(mod.download_count)}`}
                trailing={
                  <div class="btn-row">
                    <For each={mod.categories.slice(0, 2)}>{(c) => <RmTag>{c}</RmTag>}</For>
                    <RmButton size="sm" onClick={() => void openDetail(mod)}>{t("modStore.detail")}</RmButton>
                  </div>
                }
                onClick={() => void openDetail(mod)}
              />
            )}
          </For>
          <Show when={(result()?.hits.length ?? 0) === 0 && !loading()}>
            <RmEmptyState icon="search" title={t("modStore.empty")} />
          </Show>
        </div>}>
          <RmErrorState message={error() ?? ""} onRetry={() => void runSearch(page())} />
        </Show>

        <Show when={result() && result()!.total > 20}>
          <div class="pagination">
            <RmIconButton icon="chevron-left" label={t("common.prevPage")} disabled={page() <= 1} onClick={() => void runSearch(page() - 1)} />
            <span>
              {t("modStore.page", { page: page(), total: String(Math.ceil(result()!.total / 20)) })}
            </span>
            <RmIconButton
              icon="chevron-right"
              label={t("common.nextPage")}
              disabled={page() >= Math.ceil(result()!.total / 20)}
              onClick={() => void runSearch(page() + 1)}
            />
          </div>
        </Show>
      </Show>

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
                  ? version.loaders.some((l) => l.toLowerCase() === String(profile.summary.loader!.kind).toLowerCase())
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
    </div>
  );
}

function ModDetail(props: {
  mod: ModInfo;
  versions: ModVersion[];
  recommended: ModVersion | null;
  onBack: () => void;
  onInstall: (version: ModVersion) => void;
  onCopyLink: (version: ModVersion) => void;
  onOpenBrowser: (version: ModVersion) => void;
}): JSX.Element {
  // P1-14: full version list behind a toggle instead of a silent slice(0, 30).
  const [showAll, setShowAll] = createSignal(false);
  return (
    <div class="mod-detail">
      <button class="rm-btn rm-btn-ghost rm-btn-sm" onClick={props.onBack}>
        ‹ {t("modStore.backToResults")}
      </button>
      <div class="mod-detail-head">
        <div class="mod-icon is-large">
          <ModIcon url={props.mod.icon_url} name={props.mod.name} />
        </div>
        <div>
          <h3>{props.mod.name}</h3>
          <div class="muted-line">
            {props.mod.authors.join(", ")} · {abbreviateCount(props.mod.download_count)}
          </div>
          <p class="mod-description">{props.mod.description}</p>
        </div>
      </div>

      <Show when={props.recommended} keyed>
        {(version) => (
          <div class="recommended-block">
            <div class="recommended-title">⚓ {t("modStore.recommended")}</div>
            <VersionRow version={version} onInstall={() => props.onInstall(version)} onCopyLink={() => props.onCopyLink(version)} onOpenBrowser={() => props.onOpenBrowser(version)} />
          </div>
        )}
      </Show>

      <div class="rm-list">
        <For each={props.versions.slice(0, showAll() ? props.versions.length : 30)}>
          {(version) => (
            <RmListRow
              title={version.version_number}
              subtitle={`${version.game_versions.slice(0, 3).join(", ")} · ${version.loaders.join(", ")}`}
              trailing={
                <div class="btn-row">
                  <RmButton size="sm" onClick={() => props.onInstall(version)}>{t("modStore.installTo.button")}</RmButton>
                  <RmIconButton icon="copy" label={t("modStore.copyLink")} onClick={() => props.onCopyLink(version)} />
                  <RmIconButton icon="external" label={t("modStore.openInBrowser")} onClick={() => props.onOpenBrowser(version)} />
                </div>
              }
            />
          )}
        </For>
        <Show when={props.versions.length > 30}>
          <RmButton size="sm" variant="ghost" onClick={() => setShowAll(!showAll())}>
            {showAll() ? t("modStore.showLess") : t("modStore.showAll", { n: props.versions.length })}
          </RmButton>
        </Show>
      </div>
      <div class="muted-line">{t("modStore.dependencyHint")}</div>
    </div>
  );
}

function VersionRow(props: { version: ModVersion; onInstall: () => void; onCopyLink: () => void; onOpenBrowser: () => void }): JSX.Element {
  return (
    <RmListRow
      title={props.version.version_number}
      subtitle={`${props.version.game_versions.slice(0, 3).join(", ")} · ${props.version.loaders.join(", ")}`}
      trailing={
        <div class="btn-row">
          <RmButton size="sm" variant="primary" onClick={props.onInstall}>{t("modStore.installTo.button")}</RmButton>
          <RmIconButton icon="copy" label={t("modStore.copyLink")} onClick={props.onCopyLink} />
          <RmIconButton icon="external" label={t("modStore.openInBrowser")} onClick={props.onOpenBrowser} />
        </div>
      }
    />
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
