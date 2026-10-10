import {
  Component,
  createSignal,
  Show,
  For,
  onMount,
  onCleanup,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import {
  auth,
  profiles,
  versions,
  launch,
  safeInvoke,
  type AuthSession,
  type Profile,
  type VersionInfo,
  type DownloadProgressPayload,
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
    const result = await safeInvoke<AuthSession | null>("auth_get_session");
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
    const result = await safeInvoke<Profile[]>("profile_list");
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
    const result = await safeInvoke<VersionInfo[]>("version_list");
    if (result.ok) {
      setVersionList(result.data);
    } else {
      showToast(result.error);
    }
    setVersionsLoading(false);
  };

  const loadRunningStatus = async () => {
    const result = await safeInvoke<string[]>("launch_status");
    if (result.ok) {
      setRunningProfiles(result.data);
    }
  };

  onMount(async () => {
    await loadSession();
    await Promise.all([loadProfiles(), loadVersions(), loadRunningStatus()]);

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
    const result = await safeInvoke<Profile>("profile_create", {
      name,
      gameVersion,
    });
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
    const result = await safeInvoke<void>("profile_delete", { id });
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
    const result = await safeInvoke<void>("version_install", {
      version: versionId,
      profileId,
    });

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
    const result = await safeInvoke<void>("launch_start", { profileId });
    setLaunching(false);

    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleKill = async (profileId: string) => {
    const result = await safeInvoke<void>("launch_kill", { profileId });
    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleCancelDownload = async () => {
    const batchId = downloadBatchId();
    if (!batchId) return;
    const result = await safeInvoke<void>("download_cancel", { batchId });
    if (!result.ok) {
      showToast(result.error);
    }
  };

  const handleLogout = async () => {
    const result = await safeInvoke<void>("auth_logout");
    if (result.ok) {
      navigate("/login");
    } else {
      showToast(result.error);
    }
  };

  // ── Render ──

  return (
    <Show when={!authLoading()} fallback={<LoadingScreen message={t("dashboard.account.loadingSession")} />}>
      <Show when={session()} fallback={<LoadingScreen message={t("dashboard.account.redirecting")} />}>
        <div class="launcher-root">
          {/* ── Left Column: Account ── */}
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
              <div style="flex:1" />
              <button
                class="account-btn"
                onClick={handleLogout}
                style="color: var(--color-red)"
              >
                {t("dashboard.account.logout")}
              </button>
            </div>

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
          </div>

          {/* ── Center Column: Profiles + Launch ── */}
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

          {/* ── Right Column: Versions ── */}
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

export default Dashboard;
