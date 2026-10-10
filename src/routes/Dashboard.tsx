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

// ── Helpers ──

const formatTime = (iso: string | null): string => {
  if (!iso) return "Never";
  try {
    const d = new Date(iso);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return d.toLocaleDateString();
  } catch {
    return "Unknown";
  }
};

const releaseTypeLabel = (t: string): string => {
  switch (t) {
    case "release":
      return "Release";
    case "snapshot":
      return "Snapshot";
    default:
      return "Old";
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
            `\n[Game exited with code ${event.payload.exit_code}]`,
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
      }>(EVENTS.DOWNLOAD_COMPLETE, () => {
        setDownloading(false);
        setDownloadBatchId(null);
        setDownloadProgress(null);
        setInstallingVersion(null);
        loadProfiles();
        loadVersions();
        showToast({ code: "ok", message: "Version installed", retryable: false });
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
          message: event.payload.error ?? "Download failed",
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
    if (!name) return;

    setCreatingProfile(true);
    // Default to latest release version
    const latestRelease = versionList().find(
      (v) => v.release_type === "release"
    );
    const gameVersion = latestRelease?.id ?? "1.20.4";

    const result = await safeInvoke<Profile>("profile_create", {
      name,
      gameVersion,
    });
    setCreatingProfile(false);

    if (result.ok) {
      setProfileList((prev) => [...prev, result.data]);
      setSelectedProfileId(result.data.id);
      setNewProfileName("");
      setShowCreateProfile(false);
    } else {
      showToast(result.error);
    }
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

  const handleInstallVersion = async (versionId: string) => {
    const profileId = selectedProfileId();
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
    <Show when={!authLoading()} fallback={<LoadingScreen message="Loading session..." />}>
      <Show when={session()} fallback={<LoadingScreen message="Redirecting to login..." />}>
        <div class="launcher-root">
          {/* ── Left Column: Account ── */}
          <div class="col-left">
            <div class="card card-account">
              <div class="account-avatar">
                {session()!.username.charAt(0).toUpperCase()}
              </div>
              <div class="account-name">{session()!.username}</div>
              <div class="account-sub">
                {session()!.provider === "microsoft" ? "Microsoft" : "Offline"}
              </div>

              <button class="account-btn" onClick={() => setShowCreateProfile(true)}>
                + New Profile
              </button>
              <button class="account-btn" onClick={() => navigate("/settings")}>
                ⚙ Settings
              </button>
              <div style="flex:1" />
              <button
                class="account-btn"
                onClick={handleLogout}
                style="color: var(--color-red)"
              >
                Log out
              </button>
            </div>

            {/* Create Profile Modal */}
            <Show when={showCreateProfile()}>
              <div
                class="modal-overlay"
                onClick={() => setShowCreateProfile(false)}
              >
                <div class="modal-card" onClick={(e) => e.stopPropagation()}>
                  <h3 class="modal-title">Create Profile</h3>
                  <input
                    type="text"
                    class="modal-input"
                    placeholder="Profile name (e.g. Survival)"
                    value={newProfileName()}
                    onInput={(e) => setNewProfileName(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleCreateProfile();
                    }}
                    autofocus
                  />
                  <div class="modal-actions">
                    <button
                      class="modal-btn-cancel"
                      onClick={() => setShowCreateProfile(false)}
                    >
                      Cancel
                    </button>
                    <button
                      class="modal-btn-primary"
                      onClick={handleCreateProfile}
                      disabled={creatingProfile() || !newProfileName().trim()}
                    >
                      {creatingProfile() ? "Creating..." : "Create"}
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
                <span class="card-server-title">Profiles</span>
                <button
                  class="add-server-btn"
                  onClick={() => setShowCreateProfile(true)}
                >
                  + Add
                </button>
              </div>

              <Show
                when={!profilesLoading()}
                fallback={<LoadingInline message="Loading profiles..." />}
              >
                <Show
                  when={profileList().length > 0}
                  fallback={
                    <EmptyState
                      message="No profiles yet"
                      action="Create your first profile to get started"
                      onClick={() => setShowCreateProfile(true)}
                      buttonLabel="Create Profile"
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
                              MC {profile.game_version}
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
                            title="Delete profile"
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
                <div class="launch-label">Profile</div>
                <div class="launch-detail">
                  {selectedProfile()
                    ? `${selectedProfile()!.name} · MC ${selectedProfile()!.game_version}`
                    : "No profile selected"}
                </div>
              </div>
              <div class="launch-info">
                <div class="launch-label">Memory</div>
                <div class="launch-detail">
                  {selectedProfile() ? `${selectedProfile()!.memory_mb} MB` : "—"}
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
                      STOP
                    </button>
                  </Show>
                }
              >
                <button
                  class="launch-btn"
                  onClick={handleLaunch}
                  disabled={launching() || !selectedProfile()}
                >
                  {launching() ? "..." : "LAUNCH"}
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
                      Downloading version...
                    </span>
                    <button
                      onClick={handleCancelDownload}
                      style="font-size: 12px; padding: 4px 10px; border: 1px solid #3f3f46; border-radius: 6px; background: transparent; color: #d4d4d8; cursor: pointer;"
                    >
                      Cancel
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
                <span class="ver-title">Versions</span>
              </div>

              {/* Filter buttons */}
              <div class="ver-filter-row">
                <button
                  class={`ver-filter-btn ${versionFilter() === "release" ? "active" : ""}`}
                  onClick={() => setVersionFilter("release")}
                >
                  Releases
                </button>
                <button
                  class={`ver-filter-btn ${versionFilter() === "snapshot" ? "active" : ""}`}
                  onClick={() => setVersionFilter("snapshot")}
                >
                  Snapshots
                </button>
                <button
                  class={`ver-filter-btn ${versionFilter() === "all" ? "active" : ""}`}
                  onClick={() => setVersionFilter("all")}
                >
                  All
                </button>
              </div>

              <Show
                when={!versionsLoading()}
                fallback={<LoadingInline message="Loading versions..." />}
              >
                <Show
                  when={filteredVersions().length > 0}
                  fallback={<EmptyState message="No versions found" />}
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
                                ? "Installing..."
                                : isInstalled
                                ? "Installed"
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
                  <span class="ver-title">Game Output</span>
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
                    <div class="log-line log-empty">Waiting for game output...</div>
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
