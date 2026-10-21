import {
  Component,
  ErrorBoundary,
  createSignal,
  Show,
  For,
  onMount,
  onCleanup,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import {
  auth,
  dashboard,
  profiles,
  versions,
  launch,
  downloads,
  type AuthSession,
  type Profile,
  type VersionInfo,
  type DownloadProgressPayload,
  type DashboardLayout,
  type WidgetDescriptor,
  type WidgetInstance,
} from "../ipc/api";
import { showToast } from "../components/Toast";
import { EVENTS } from "../ipc/events";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../i18n";

// ── Helpers ──

const formatTime = (iso: string | null): string => {
  if (!iso) return t("dashboard.time.never");
  try {
    const d = new Date(iso);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return t("dashboard.time.justNow");
    if (mins < 60) return t("dashboard.time.minutesAgo", { count: mins });
    const hours = Math.floor(mins / 60);
    if (hours < 24) return t("dashboard.time.hoursAgo", { count: hours });
    return d.toLocaleDateString();
  } catch {
    return t("dashboard.time.unknown");
  }
};

const releaseTypeLabel = (releaseType: string): string => {
  switch (releaseType) {
    case "release":
      return t("dashboard.version.release");
    case "snapshot":
      return t("dashboard.version.snapshot");
    default:
      return t("dashboard.version.old");
  }
};

// ── Dashboard ──

const Dashboard: Component = () => {
  const navigate = useNavigate();

  // ── Auth state ──
  const [session, setSession] = createSignal<AuthSession | null>(null);
  const [authLoading, setAuthLoading] = createSignal(true);

  // ── Profile state ──
  const [profileList, setProfileList] = createSignal<Profile[]>([]);
  const [profilesLoading, setProfilesLoading] = createSignal(true);
  const [selectedProfileId, setSelectedProfileId] = createSignal<string | null>(null);
  const [newProfileName, setNewProfileName] = createSignal("");
  const [showCreateProfile, setShowCreateProfile] = createSignal(false);

  // ── Dashboard layout (Phase 3, ADR-005) ──
  const [layout, setLayout] = createSignal<DashboardLayout | null>(null);
  const [breakpoint, setBreakpoint] = createSignal<"wide" | "medium" | "compact">("wide");
  const [savingLayout, setSavingLayout] = createSignal(false);
  const [draggingId, setDraggingId] = createSignal<string | null>(null);
  const [widgetDescriptors, setWidgetDescriptors] = createSignal<WidgetDescriptor[]>([]);
  let gridRef: HTMLDivElement | undefined;
  const [creatingProfile, setCreatingProfile] = createSignal(false);
  /// Version picked in the create dialog. Creating an instance without an
  /// explicit version is not allowed — the version determines what gets
  /// downloaded, so it must be a visible choice rather than a silent default.
  const [createVersion, setCreateVersion] = createSignal("");
  const [createIncludeSnapshots, setCreateIncludeSnapshots] = createSignal(false);

  // ── Version state ──
  const [versionList, setVersionList] = createSignal<VersionInfo[]>([]);
  const [versionsLoading, setVersionsLoading] = createSignal(true);
  const [installingVersion, setInstallingVersion] = createSignal<string | null>(null);
  const [versionFilter, setVersionFilter] = createSignal("release");

  // ── Launch state ──
  const [launching, setLaunching] = createSignal(false);
  const [runningProfiles, setRunningProfiles] = createSignal<string[]>([]);
  const [gameLog, setGameLog] = createSignal<string[]>([]);
  const [showLog, setShowLog] = createSignal(false);
  let logContainerRef: HTMLDivElement | undefined;

  // ── Download state (P0-04) ──
  const [downloading, setDownloading] = createSignal(false);
  const [downloadBatchId, setDownloadBatchId] = createSignal<string | null>(null);
  const [downloadProgress, setDownloadProgress] = createSignal<{
    percent: number;
    current: string | null;
  } | null>(null);

  // ── Event unlisteners ──
  let unlisteners: UnlistenFn[] = [];

  // ── Derived ──
  const selectedProfile = () =>
    profileList().find((p) => p.id === selectedProfileId()) ?? null;

  const filteredVersions = () => {
    const all = versionList();
    if (versionFilter() === "all") return all;
    if (versionFilter() === "snapshot")
      return all.filter((v) => v.release_type === "snapshot");
    return all.filter((v) => v.release_type === "release");
  };

  /// Versions offered in the create dialog; releases only unless the user opts
  /// into snapshots (the full list is ~900 entries).
  const createVersionOptions = () =>
    versionList().filter(
      (v) => createIncludeSnapshots() || v.release_type === "release"
    );

  const isProfileRunning = (id: string) => runningProfiles().includes(id);

  // ── Load data ──

  const loadSession = async () => {
    setAuthLoading(true);
    const result = await auth.getSession();
    if (result.ok) {
      setSession(result.data);
      if (!result.data) {
        navigate("/login");
      }
    } else {
      showToast(result.error);
    }
    setAuthLoading(false);
  };

  const loadProfiles = async () => {
    setProfilesLoading(true);
    const result = await profiles.list();
    if (result.ok) {
      setProfileList(result.data);
      if (result.data.length > 0 && !selectedProfileId()) {
        setSelectedProfileId(result.data[0].id);
      }
    } else {
      showToast(result.error);
    }
    setProfilesLoading(false);
  };

  const loadVersions = async () => {
    setVersionsLoading(true);
    const result = await versions.list();
    if (result.ok) {
      setVersionList(result.data);
    } else {
      showToast(result.error);
    }
    setVersionsLoading(false);
  };

  const loadRunningStatus = async () => {
    const result = await launch.status();
    if (result.ok) {
      setRunningProfiles(result.data);
    }
  };

  onMount(async () => {
    await loadSession();
    await Promise.all([loadProfiles(), loadVersions(), loadRunningStatus()]);
    updateBreakpoint();
    window.addEventListener("resize", updateBreakpoint);
    await loadDashboardLayout();

    // Listen for launch events
    try {
      const u1 = await listen<{ profile_id: string; pid: number }>(
        EVENTS.LAUNCH_STARTED,
        (event) => {
          setRunningProfiles((prev) => [...prev, event.payload.profile_id]);
          setGameLog([]);
          setShowLog(true);
        }
      );
      unlisteners.push(u1);

      const u2 = await listen<{ profile_id: string; text: string }>(
        EVENTS.LAUNCH_OUTPUT,
        (event) => {
          if (event.payload.profile_id === selectedProfileId()) {
            setGameLog((prev) => {
              const next = [...prev, event.payload.text];
              if (next.length > 500) return next.slice(-500);
              return next;
            });
            // Auto-scroll
            setTimeout(() => {
              if (logContainerRef) {
                logContainerRef.scrollTop = logContainerRef.scrollHeight;
              }
            }, 10);
          }
        }
      );
      unlisteners.push(u2);

      const u3 = await listen<{ profile_id: string; exit_code: number }>(
        EVENTS.LAUNCH_EXITED,
        (event) => {
          setRunningProfiles((prev) =>
            prev.filter((id) => id !== event.payload.profile_id)
          );
          setGameLog((prev) => [
            ...prev,
            `\n${t("dashboard.output.exited", { code: event.payload.exit_code })}`,
          ]);
          // Reload profiles to update last_played
          loadProfiles();
        }
      );
      unlisteners.push(u3);

      // Listen for download events (version install, P0-04)
      const u4 = await listen<DownloadProgressPayload>(
        EVENTS.DOWNLOAD_PROGRESS,
        (event) => {
          const p = event.payload;
          setDownloading(true);
          setDownloadBatchId(p.batch_id || null);
          setDownloadProgress({
            percent:
              p.total_files > 0
                ? Math.min(100, Math.round((p.completed_files / p.total_files) * 100))
                : 0,
            current: p.current_file,
          });
        }
      );
      unlisteners.push(u4);

      const u5 = await listen<{
        batch_id: string;
        profile_id: string;
        version: string;
      }>(EVENTS.DOWNLOAD_COMPLETE, (event) => {
        setDownloading(false);
        setDownloadBatchId(null);
        setDownloadProgress(null);
        setInstallingVersion(null);
        loadProfiles();
        loadVersions();
        showToast({
          code: "ok",
          message: t("dashboard.version.installed", { version: event.payload.version }),
          retryable: false,
        });
      });
      unlisteners.push(u5);

      const u6 = await listen<{
        batch_id: string;
        profile_id: string;
        version: string;
        error?: string;
      }>(EVENTS.DOWNLOAD_ERROR, (event) => {
        setDownloading(false);
        setDownloadBatchId(null);
        setDownloadProgress(null);
        setInstallingVersion(null);
        showToast({
          code: "error",
          message: event.payload.error ?? t("dashboard.version.downloadFailed"),
          retryable: false,
        });
      });
      unlisteners.push(u6);
    } catch (e) {
      console.error("Failed to register event listeners:", e);
    }
  });

  onCleanup(() => {
    window.removeEventListener("resize", updateBreakpoint);
    unlisteners.forEach((u) => u());
  });

  // ── Actions ──

  const handleCreateProfile = async () => {
    const name = newProfileName().trim();
    const gameVersion = createVersion();
    // Both are required: the version decides what gets downloaded, so it is an
    // explicit choice rather than an invisible default.
    if (!name || !gameVersion) return;

    setCreatingProfile(true);
    const result = await profiles.create(name, gameVersion);
    setCreatingProfile(false);

    if (!result.ok) {
      showToast(result.error);
      return;
    }

    setProfileList((prev) => [...prev, result.data]);
    setSelectedProfileId(result.data.id);
    setNewProfileName("");
    setCreateVersion("");
    setShowCreateProfile(false);

    // Creating an instance immediately starts downloading its version, so the
    // user lands on a usable instance instead of having to pick the version a
    // second time in the versions panel.
    await handleInstallVersion(gameVersion, result.data.id);
  };

  const handleDeleteProfile = async (id: string) => {
    const result = await profiles.delete(id);
    if (result.ok) {
      setProfileList((prev) => prev.filter((p) => p.id !== id));
      if (selectedProfileId() === id) {
        const remaining = profileList().filter((p) => p.id !== id);
        setSelectedProfileId(remaining.length > 0 ? remaining[0].id : null);
      }
    } else {
      showToast(result.error);
    }
  };

  /// Installs `versionId` for `profileIdOverride`, or for the selected profile
  /// when no override is given (used right after creating an instance, before
  /// the selection signal has propagated).
  const handleInstallVersion = async (
    versionId: string,
    profileIdOverride?: string
  ) => {
    const profileId = profileIdOverride ?? selectedProfileId();
    if (!profileId) return;

    setInstallingVersion(versionId);
    setDownloading(true);
    const result = await versions.install(versionId, profileId);

    if (!result.ok) {
      setInstallingVersion(null);
      setDownloading(false);
      showToast(result.error);
    }
    // 成功后 UI 更新依赖 download://complete 事件（P0-04 后台任务化）
  };

  const handleLaunch = async () => {
    const profileId = selectedProfileId();
    if (!profileId) return;

    setLaunching(true);
    const result = await launch.start(profileId);
    setLaunching(false);

    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleKill = async (profileId: string) => {
    const result = await launch.kill(profileId);
    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleCancelDownload = async () => {
    const batchId = downloadBatchId();
    if (!batchId) return;
    const result = await downloads.cancel(batchId);
    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleLogout = async () => {
    const result = await auth.logout();
    if (result.ok) {
      navigate("/login");
    } else {
      showToast(result.error);
    }
  };

  // ── Layout logic (Phase 3) ──

  const loadDashboardLayout = async () => {
    const [layoutRes, widgetsRes] = await Promise.all([
      dashboard.getLayout(),
      dashboard.listWidgets(),
    ]);
    if (layoutRes.ok) setLayout(layoutRes.data);
    else showToast(layoutRes.error);
    if (widgetsRes.ok) setWidgetDescriptors(widgetsRes.data);
  };

  const updateBreakpoint = () => {
    const width = window.innerWidth;
    setBreakpoint(width >= 1280 ? "wide" : width >= 768 ? "medium" : "compact");
  };

  const effectiveColumns = () => {
    const cols = layout()?.grid.columns ?? 12;
    if (breakpoint() === "medium") return Math.min(cols, 8);
    if (breakpoint() === "compact") return Math.min(cols, 4);
    return cols;
  };

  const widgetSize = (w: WidgetInstance) => {
    const override =
      breakpoint() === "medium"
        ? w.breakpointOverrides?.medium
        : breakpoint() === "compact"
          ? w.breakpointOverrides?.compact
          : undefined;
    const size = override ?? w.size;
    return {
      w: Math.max(1, Math.min(size.w, effectiveColumns())),
      h: Math.max(1, size.h),
    };
  };

  const visibleWidgets = () => (layout()?.widgets ?? []).filter((w) => w.enabled);

  const gridStyle = () => {
    const gap = layout()?.grid.gap ?? 16;
    return `display: grid; grid-template-columns: repeat(${effectiveColumns()}, minmax(0, 1fr)); gap: ${gap}px; grid-auto-rows: 120px;`;
  };

  const slotStyle = (w: WidgetInstance) => {
    const size = widgetSize(w);
    return `grid-column: ${w.position.x + 1} / span ${size.w}; grid-row: ${w.position.y + 1} / span ${size.h};`;
  };

  const widgetLabel = (w: WidgetInstance) => {
    const key = `${w.pluginId}/${w.widgetId}`;
    if (key === "org.rmccc.core-ui/account-card") return t("dashboard.account.title");
    if (key === "org.rmccc.core-ui/instance-launcher") return t("dashboard.profile.title");
    if (key === "org.rmccc.core-ui/version-browser") return t("dashboard.version.title");
    if (w.widgetId.startsWith("dashboard-card:")) {
      return w.widgetId.slice("dashboard-card:".length);
    }
    return w.widgetId;
  };

  const persistLayout = async (next: DashboardLayout) => {
    setSavingLayout(true);
    const res = await dashboard.saveLayout(next);
    setSavingLayout(false);
    if (res.ok) setLayout(res.data.layout);
    else showToast(res.error);
  };

  const handleDragStart = (e: DragEvent, w: WidgetInstance) => {
    setDraggingId(w.instanceId);
    e.dataTransfer?.setData("text/plain", w.instanceId);
  };

  const handleDrop = async (e: DragEvent) => {
    e.preventDefault();
    const id = draggingId() ?? e.dataTransfer?.getData("text/plain");
    if (!id || !gridRef || !layout()) return;
    const rect = gridRef.getBoundingClientRect();
    const gap = layout()!.grid.gap;
    const cols = effectiveColumns();
    const cellW = (rect.width - gap * (cols - 1)) / cols;
    const rowH = 120 + gap;
    const x = Math.max(
      0,
      Math.min(cols - 1, Math.floor((e.clientX - rect.left) / (cellW + gap)))
    );
    const y = Math.max(0, Math.floor((e.clientY - rect.top) / rowH));
    const next: DashboardLayout = {
      ...layout()!,
      widgets: layout()!.widgets.map((w) =>
        w.instanceId === id ? { ...w, position: { x, y } } : w
      ),
    };
    setDraggingId(null);
    await persistLayout(next);
  };

  const cycleWidgetWidth = async (w: WidgetInstance) => {
    const widths = [1, 2, 3, 4, 6, 8, 12];
    const nextW = widths.find((n) => n > w.size.w) ?? widths[0];
    const next: DashboardLayout = {
      ...layout()!,
      widgets: layout()!.widgets.map((item) =>
        item.instanceId === w.instanceId
          ? { ...item, size: { ...item.size, w: nextW } }
          : item
      ),
    };
    await persistLayout(next);
  };

  const hideWidget = async (w: WidgetInstance) => {
    const next: DashboardLayout = {
      ...layout()!,
      widgets: layout()!.widgets.map((item) =>
        item.instanceId === w.instanceId ? { ...item, enabled: false } : item
      ),
    };
    await persistLayout(next);
  };

  const handleResetLayout = async () => {
    const res = await dashboard.resetLayout();
    if (res.ok) setLayout(res.data);
    else showToast(res.error);
  };

  const restoreHiddenWidgets = async () => {
    if (!layout()) return;
    const next: DashboardLayout = {
      ...layout()!,
      widgets: layout()!.widgets.map((w) => ({ ...w, enabled: true })),
    };
    await persistLayout(next);
  };

  const addMissingWidgets = async () => {
    if (!layout()) return;
    const existing = new Set(
      layout()!.widgets.map((w) => `${w.pluginId}/${w.widgetId}`)
    );
    const missing = widgetDescriptors().filter(
      (d) => !existing.has(`${d.pluginId}/${d.widgetId}`)
    );
    if (missing.length === 0) return;
    const maxY = layout()!.widgets.reduce(
      (acc, w) => Math.max(acc, w.position.y + w.size.h),
      0
    );
    const additions = missing.map((d, index) => ({
      instanceId: `${d.pluginId}-${d.widgetId}`
        .replace(/[^a-z0-9-]/g, "-")
        .slice(0, 64),
      pluginId: d.pluginId,
      widgetId: d.widgetId,
      enabled: true,
      position: { x: 0, y: maxY + index * 2 },
      size: {
        w: Math.min(d.defaultSize.w, layout()!.grid.columns),
        h: d.defaultSize.h,
      },
      breakpointOverrides: { medium: null, compact: null },
      configuration: {},
    }));
    await persistLayout({
      ...layout()!,
      widgets: [...layout()!.widgets, ...additions],
    });
  };

  // ── Built-in widget panels (reuse the existing markup) ──

  const accountPanel = () => (
          <div class="col-left">
            <div class="card card-account">
              <div class="account-avatar">
                {session()!.username.charAt(0).toUpperCase()}
              </div>
              <div class="account-name">{session()!.username}</div>
              <div class="account-sub">
                {session()!.provider === "microsoft" ? t("dashboard.account.providerMicrosoft") : t("dashboard.account.providerOffline")}
              </div>

              <button class="account-btn" onClick={() => setShowCreateProfile(true)}>
                {t("dashboard.account.newProfile")}
              </button>
              <button class="account-btn" onClick={() => navigate("/settings")}>
                {t("dashboard.account.settings")}
              </button>
              <button class="account-btn" onClick={() => navigate("/plugins")}>
                {t("plugins.title")}
              </button>
              <div style="flex:1" />
              <button
                class="account-btn"
                onClick={handleLogout}
                style="color: var(--color-red)"
              >
                {t("dashboard.account.logout")}
              </button>
            </div>


          </div>
  );

  const profilesPanel = () => (
          <div class="col-center">
            <div class="card card-server">
              <div class="card-server-header">
                <span class="card-server-title">{t("dashboard.profile.title")}</span>
                <button
                  class="add-server-btn"
                  onClick={() => setShowCreateProfile(true)}
                >
                  {t("dashboard.profile.add")}
                </button>
                <button
                  class="add-server-btn"
                  onClick={() => navigate("/mods")}
                >
                  {t("mods.title")}
                </button>
              </div>

              <Show
                when={!profilesLoading()}
                fallback={<LoadingInline message={t("dashboard.profile.loading")} />}
              >
                <Show
                  when={profileList().length > 0}
                  fallback={
                    <EmptyState
                      message={t("dashboard.profile.empty")}
                      action={t("dashboard.profile.emptyAction")}
                      onClick={() => setShowCreateProfile(true)}
                      buttonLabel={t("dashboard.profile.emptyButton")}
                    />
                  }
                >
                  <div class="server-list">
                    <For each={profileList()}>
                      {(profile) => (
                        <div
                          class={`server-row ${
                            selectedProfileId() === profile.id ? "selected" : ""
                          }`}
                          onClick={() => setSelectedProfileId(profile.id)}
                        >
                          <div
                            class={`server-icon ${
                              isProfileRunning(profile.id) ? "s2" : "s1"
                            }`}
                          >
                            {isProfileRunning(profile.id) ? "▶" : "▷"}
                          </div>
                          <div class="server-info">
                            <div class="server-name">{profile.name}</div>
                            <div class="server-addr">
                              {t("dashboard.profile.gameVersion", { version: profile.game_version })}
                              {profile.mod_loader
                                ? ` · ${profile.mod_loader.loader_type}`
                                : ""}
                            </div>
                          </div>
                          <div class="server-ping">
                            <div class="ping-unit">
                              {formatTime(profile.last_played)}
                            </div>
                          </div>
                          <button
                            class="profile-settings-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              navigate(`/instances/${profile.id}`);
                            }}
                            title={t("dashboard.profile.settings")}
                            aria-label={t("dashboard.profile.settings")}
                          >
                            ⚙
                          </button>
                          <button
                            class="profile-delete-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDeleteProfile(profile.id);
                            }}
                            title={t("dashboard.profile.delete")}
                          >
                            ×
                          </button>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>

              <div class="brand-row">
                <span class="brand-logo">RMCCC</span>
              </div>
            </div>

            {/* ── Launch Bar ── */}
            <div class="card-launch">
              <div class="launch-info">
                <div class="launch-label">{t("dashboard.launch.profile")}</div>
                <div class="launch-detail">
                  {selectedProfile()
                    ? t("dashboard.launch.profileDetail", {
                        name: selectedProfile()!.name,
                        version: selectedProfile()!.game_version,
                      })
                    : t("dashboard.launch.noProfile")}
                </div>
              </div>
              <div class="launch-info">
                <div class="launch-label">{t("dashboard.launch.memory")}</div>
                <div class="launch-detail">
                  {selectedProfile()
                    ? t("dashboard.launch.memoryValue", { size: selectedProfile()!.memory_mb })
                    : "—"}
                </div>
              </div>
              <Show
                when={
                  selectedProfile() && !isProfileRunning(selectedProfileId()!)
                }
                fallback={
                  <Show when={selectedProfile() && isProfileRunning(selectedProfileId()!)}>
                    <button
                      class="launch-btn"
                      style="background: var(--color-red); color: #fff"
                      onClick={() => handleKill(selectedProfileId()!)}
                    >
                      {t("dashboard.launch.stop")}
                    </button>
                  </Show>
                }
              >
                <button
                  class="launch-btn"
                  onClick={handleLaunch}
                  disabled={launching() || !selectedProfile()}
                >
                  {launching() ? "..." : t("dashboard.launch.start")}
                </button>
              </Show>
            </div>
  
              {/* ── Download Progress (P0-04) ── */}
              <Show when={downloading()}>
                <div class="card" style="margin-top: 12px; padding: 12px;">
                  <div
                    style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;"
                  >
                    <span style="font-size: 13px; color: #a1a1aa;">
                      {t("dashboard.version.downloading")}
                    </span>
                    <button
                      onClick={handleCancelDownload}
                      style="font-size: 12px; padding: 4px 10px; border: 1px solid #3f3f46; border-radius: 6px; background: transparent; color: #d4d4d8; cursor: pointer;"
                    >
                      {t("common.cancel")}
                    </button>
                  </div>
                  <div
                    style="height: 6px; background: #27272a; border-radius: 999px; overflow: hidden;"
                  >
                    <div
                      style={`height: 100%; width: ${downloadProgress()?.percent ?? 0}%; background: linear-gradient(90deg, #3b82f6, #22d3ee); transition: width 0.15s ease;`}
                    />
                  </div>
                  <div
                    style="display: flex; justify-content: space-between; margin-top: 6px; font-size: 11px; color: #71717a;"
                  >
                    <span>{downloadProgress()?.percent ?? 0}%</span>
                    <span>{downloadProgress()?.current ?? ""}</span>
                  </div>
                </div>
              </Show>
          </div>
  );

  const versionsPanel = () => (
          <div class="col-right">
            <div class="card card-version">
              <div class="ver-header">
                <span class="ver-title">{t("dashboard.version.title")}</span>
              </div>

              {/* Filter buttons */}
              <div class="ver-filter-row">
                <button
                  class={`ver-filter-btn ${versionFilter() === "release" ? "active" : ""}`}
                  onClick={() => setVersionFilter("release")}
                >
                  {t("dashboard.version.releases")}
                </button>
                <button
                  class={`ver-filter-btn ${versionFilter() === "snapshot" ? "active" : ""}`}
                  onClick={() => setVersionFilter("snapshot")}
                >
                  {t("dashboard.version.snapshots")}
                </button>
                <button
                  class={`ver-filter-btn ${versionFilter() === "all" ? "active" : ""}`}
                  onClick={() => setVersionFilter("all")}
                >
                  {t("dashboard.version.all")}
                </button>
              </div>

              <Show
                when={!versionsLoading()}
                fallback={<LoadingInline message={t("dashboard.version.loading")} />}
              >
                <Show
                  when={filteredVersions().length > 0}
                  fallback={<EmptyState message={t("dashboard.version.empty")} />}
                >
                  <div class="ver-list">
                    <For each={filteredVersions().slice(0, 100)}>
                      {(v) => {
                        const isInstalled =
                          selectedProfile()?.game_version === v.id;
                        const isInstalling = installingVersion() === v.id;
                        return (
                          <div
                            class={`ver-row ${isInstalled ? "active" : ""}`}
                            onClick={() => handleInstallVersion(v.id)}
                          >
                            <div
                              class={`ver-badge ${
                                isInstalled ? "fabric" : "vanilla"
                              }`}
                            >
                              {isInstalling ? "⋯" : isInstalled ? "✓" : releaseTypeLabel(v.release_type).charAt(0)}
                            </div>
                            <div class="ver-name">{v.id}</div>
                            <div class="ver-meta">
                              {isInstalling
                                ? t("dashboard.version.installing")
                                : isInstalled
                                ? t("dashboard.version.installedBadge")
                                : releaseTypeLabel(v.release_type)}
                            </div>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </Show>
            </div>

            {/* ── Game Log Panel ── */}
            <Show when={showLog()}>
              <div class="card card-log">
                <div class="card-log-header">
                  <span class="ver-title">{t("dashboard.output.title")}</span>
                  <button
                    class="log-close-btn"
                    onClick={() => setShowLog(false)}
                  >
                    ×
                  </button>
                </div>
                <div class="log-output" ref={logContainerRef}>
                  <For each={gameLog()}>
                    {(line) => <div class="log-line">{line}</div>}
                  </For>
                  <Show when={gameLog().length === 0}>
                    <div class="log-line log-empty">{t("dashboard.output.waiting")}</div>
                  </Show>
                </div>
              </div>
            </Show>
          </div>
  );

  const renderWidget = (w: WidgetInstance) => {
    const key = `${w.pluginId}/${w.widgetId}`;
    switch (key) {
      case "org.rmccc.core-ui/account-card":
        return accountPanel();
      case "org.rmccc.core-ui/instance-launcher":
        return profilesPanel();
      case "org.rmccc.core-ui/version-browser":
        return versionsPanel();
      default: {
        if (w.widgetId.startsWith("dashboard-card:")) {
          const descriptor = widgetDescriptors().find(
            (d) => d.pluginId === w.pluginId && d.widgetId === w.widgetId
          );
          if (descriptor) {
            return <PluginDashCard widget={w} descriptor={descriptor} />;
          }
        }
        return <UnknownWidget widget={w} />;
      }
    }
  };

  const createProfileModal = () => (
    <>
            {/* Create Profile Modal */}
            <Show when={showCreateProfile()}>
              <div
                class="modal-overlay"
                onClick={() => setShowCreateProfile(false)}
              >
                <div class="modal-card" onClick={(e) => e.stopPropagation()}>
                  <h3 class="modal-title">{t("dashboard.profile.create.title")}</h3>
                  <input
                    type="text"
                    class="modal-input"
                    placeholder={t("dashboard.profile.create.placeholder")}
                    value={newProfileName()}
                    onInput={(e) => setNewProfileName(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleCreateProfile();
                    }}
                    autofocus
                  />

                  {/* Game version is mandatory: it decides what gets downloaded
                      right after the instance is created. */}
                  <label class="modal-field-label" for="create-version">
                    {t("dashboard.profile.create.versionLabel")}
                  </label>
                  <select
                    id="create-version"
                    class="modal-input"
                    onChange={(e) => setCreateVersion(e.currentTarget.value)}
                  >
                    <option value="" selected={createVersion() === ""}>
                      {t("dashboard.profile.create.versionPlaceholder")}
                    </option>
                    <For each={createVersionOptions()}>
                      {(v) => (
                        <option value={v.id} selected={createVersion() === v.id}>
                          {v.id}
                        </option>
                      )}
                    </For>
                  </select>
                  <label class="modal-checkbox">
                    <input
                      type="checkbox"
                      checked={createIncludeSnapshots()}
                      onChange={(e) =>
                        setCreateIncludeSnapshots(e.currentTarget.checked)
                      }
                    />
                    {t("dashboard.profile.create.includeSnapshots")}
                  </label>

                  <div class="modal-actions">
                    <button
                      class="modal-btn-cancel"
                      onClick={() => setShowCreateProfile(false)}
                    >
                      {t("common.cancel")}
                    </button>
                    <button
                      class="modal-btn-primary"
                      onClick={handleCreateProfile}
                      disabled={
                        creatingProfile() ||
                        !newProfileName().trim() ||
                        !createVersion()
                      }
                    >
                      {creatingProfile() ? t("dashboard.profile.create.busy") : t("dashboard.profile.create.submit")}
                    </button>
                  </div>
                </div>
              </div>
            </Show>
    </>
  );

  // ── Render ──

  return (
    <Show when={!authLoading()} fallback={<LoadingScreen message={t("dashboard.account.loadingSession")} />}>
      <Show when={session()} fallback={<LoadingScreen message={t("dashboard.account.redirecting")} />}>
        <div class="dashboard-page">
          <div class="dashboard-toolbar">
            <span class="dashboard-hint">{t("dashboard.layout.hint")}</span>
            <div class="dashboard-toolbar-actions">
              <button
                class="settings-refresh-btn"
                onClick={addMissingWidgets}
                disabled={savingLayout()}
              >
                {t("dashboard.layout.addWidgets")}
              </button>
              <button
                class="settings-refresh-btn"
                onClick={restoreHiddenWidgets}
                disabled={savingLayout()}
              >
                {t("dashboard.layout.restoreHidden")}
              </button>
              <button
                class="settings-refresh-btn"
                onClick={handleResetLayout}
                disabled={savingLayout()}
              >
                {t("dashboard.layout.reset")}
              </button>
            </div>
          </div>
          <div
            class="dashboard-grid"
            ref={(el) => (gridRef = el)}
            style={gridStyle()}
            onDragOver={(e) => e.preventDefault()}
            onDrop={handleDrop}
          >
            <For each={visibleWidgets()}>
              {(w) => (
                <div class="dashboard-slot" style={slotStyle(w)}>
                  <div class="dashboard-widget-head">
                    <span
                      class="dashboard-drag"
                      draggable={true}
                      onDragStart={(e) => handleDragStart(e, w)}
                      title={t("dashboard.layout.drag")}
                    >
                      ⠿
                    </span>
                    <span class="dashboard-widget-label">{widgetLabel(w)}</span>
                    <button
                      class="dashboard-widget-btn"
                      onClick={() => cycleWidgetWidth(w)}
                      title={t("dashboard.layout.resize")}
                    >
                      ↔
                    </button>
                    <button
                      class="dashboard-widget-btn"
                      onClick={() => hideWidget(w)}
                      title={t("dashboard.layout.hide")}
                    >
                      ×
                    </button>
                  </div>
                  <ErrorBoundary
                    fallback={(err) => <WidgetError message={String(err)} />}
                  >
                    {renderWidget(w)}
                  </ErrorBoundary>
                </div>
              )}
            </For>
          </div>
          {createProfileModal()}
        </div>
      </Show>
    </Show>
  );
};

// ── Sub-components ──

const LoadingScreen: Component<{ message: string }> = (props) => (
  <div class="loading-screen">
    <div class="loading-spinner" />
    <div class="loading-text">{props.message}</div>
  </div>
);

const LoadingInline: Component<{ message: string }> = (props) => (
  <div class="loading-inline">
    <div class="loading-spinner-small" />
    <span>{props.message}</span>
  </div>
);

const EmptyState: Component<{
  message: string;
  action?: string;
  onClick?: () => void;
  buttonLabel?: string;
}> = (props) => (
  <div class="empty-state">
    <div class="empty-icon">📦</div>
    <div class="empty-message">{props.message}</div>
    <Show when={props.action}>
      <div class="empty-action">{props.action}</div>
    </Show>
    <Show when={props.onClick && props.buttonLabel}>
      <button class="empty-btn" onClick={props.onClick}>
        {props.buttonLabel}
      </button>
    </Show>
  </div>
);

// ── Plugin / placeholder widgets (Phase 3) ──

const PluginDashCard: Component<{
  widget: WidgetInstance;
  descriptor: WidgetDescriptor;
}> = (props) => (
  <div class="card dashboard-plugin-card">
    <div class="dashboard-plugin-title">{props.descriptor.displayName}</div>
    <p class="dashboard-plugin-desc">{t("dashboard.pluginCard.placeholder")}</p>
  </div>
);

const UnknownWidget: Component<{ widget: WidgetInstance }> = (props) => (
  <div class="card dashboard-plugin-card is-missing">
    <div class="dashboard-plugin-title">{props.widget.widgetId}</div>
    <p class="dashboard-plugin-desc">{t("dashboard.pluginCard.unavailable")}</p>
  </div>
);

const WidgetError: Component<{ message: string }> = (props) => (
  <div class="card dashboard-plugin-card is-error">
    <div class="dashboard-plugin-title">{t("dashboard.widget.error")}</div>
    <p class="dashboard-plugin-desc">{props.message}</p>
  </div>
);

export default Dashboard;
