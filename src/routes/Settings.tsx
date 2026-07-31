import { Component, createSignal, onMount, Show, For } from "solid-js";
import { useNavigate } from "@solidjs/router";
import {
  settings,
  safeInvoke,
  type AppConfig,
  type JavaInfo,
  type SettingsUpdate,
} from "../ipc/api";
import { showToast } from "../components/Toast";

const Settings: Component = () => {
  const navigate = useNavigate();

  // ── State ──
  const [config, setConfig] = createSignal<AppConfig | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);

  const [gameRootDir, setGameRootDir] = createSignal("");
  const [javaPath, setJavaPath] = createSignal("");
  const [minMemory, setMinMemory] = createSignal(256);
  const [maxMemory, setMaxMemory] = createSignal(4096);

  const [javaInstallations, setJavaInstallations] = createSignal<JavaInfo[]>([]);
  const [detectingJava, setDetectingJava] = createSignal(false);

  onMount(async () => {
    const result = await safeInvoke<AppConfig>("settings_get");
    if (result.ok) {
      const c = result.data;
      setConfig(c);
      setGameRootDir(c.game_root_dir ?? "");
      setJavaPath(c.default_java_path ?? "");
      setMinMemory(c.default_min_memory_mb);
      setMaxMemory(c.default_max_memory_mb);
    } else {
      showToast(result.error);
    }
    setLoading(false);
    await detectJava();
  });

  const detectJava = async () => {
    setDetectingJava(true);
    const result = await safeInvoke<JavaInfo[]>("java_detect");
    if (result.ok) {
      setJavaInstallations(result.data);
    } else {
      showToast(result.error);
    }
    setDetectingJava(false);
  };

  const handleSave = async () => {
    setSaving(true);

    const opts: SettingsUpdate = {
      game_root_dir: gameRootDir().trim() || null,
      default_java_path: javaPath().trim() || null,
      default_min_memory_mb: minMemory(),
      default_max_memory_mb: maxMemory(),
    };

    const result = await safeInvoke<AppConfig>("settings_set", {
      opts,
    });
    setSaving(false);

    if (result.ok) {
      setConfig(result.data);
      showToast({
        code: "success",
        message: "Settings saved successfully",
        retryable: false,
      });
    } else {
      showToast(result.error);
    }
  };

  const selectJava = (path: string) => {
    setJavaPath(path);
  };

  // ── Render ──
  return (
    <div class="settings-page">
      <div class="settings-header">
        <button class="settings-back-btn" onClick={() => navigate("/")}>
          ← Back
        </button>
        <h2 class="settings-title">Settings</h2>
      </div>

      <Show
        when={!loading()}
        fallback={
          <div class="loading-inline">
            <div class="loading-spinner-small" />
            <span>Loading settings...</span>
          </div>
        }
      >
        <div class="settings-content">
          {/* ── Game Directory ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">Game Directory</h3>
            <p class="settings-section-desc">
              Root directory for all Minecraft data — versions, assets,
              libraries, and mods. Follows Mojang standard layout.
            </p>

            <div class="settings-field">
              <label class="settings-label">
                Game Root (.minecraft equivalent)
                <span class="settings-hint">
                  Absolute path. Leave empty for default ({config()?.game_root_dir || "{data}/instances"}).
                </span>
              </label>
              <input
                type="text"
                class="settings-input settings-input-mono"
                placeholder="/path/to/minecraft"
                value={gameRootDir()}
                onInput={(e) => setGameRootDir(e.currentTarget.value)}
              />
            </div>
          </div>

          {/* ── Java Runtime ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">
              Java Runtime
              <button
                class="settings-refresh-btn"
                onClick={detectJava}
                disabled={detectingJava()}
              >
                {detectingJava() ? "Detecting..." : "Re-detect"}
              </button>
            </h3>

            <Show
              when={javaInstallations().length > 0}
              fallback={
                <div class="settings-empty">
                  No Java installations detected. Install Java 8+ to run
                  Minecraft.
                </div>
              }
            >
              <div class="java-list">
                <For each={javaInstallations()}>
                  {(java) => (
                    <div
                      class={`java-item ${javaPath() === java.path ? "selected" : ""}`}
                      onClick={() => selectJava(java.path)}
                    >
                      <div class="java-version-badge">
                        Java {java.major_version}
                      </div>
                      <div class="java-info">
                        <div class="java-path">{java.path}</div>
                        <div class="java-meta">
                          {java.vendor}
                          {java.is_64bit ? " · 64-bit" : " · 32-bit"}
                        </div>
                      </div>
                      <Show when={javaPath() === java.path}>
                        <div class="java-check">✓</div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <div class="settings-field" style="margin-top: 12px">
              <label class="settings-label">
                Custom Java Path
                <span class="settings-hint">
                  Or manually specify the Java executable
                </span>
              </label>
              <input
                type="text"
                class="settings-input settings-input-mono"
                placeholder="/usr/bin/java"
                value={javaPath()}
                onInput={(e) => setJavaPath(e.currentTarget.value)}
              />
            </div>
          </div>

          {/* ── Memory ── */}
          <div class="settings-section">
            <h3 class="settings-section-title">JVM Memory</h3>
            <p class="settings-section-desc">
              Default heap size for all profiles. Can be overridden per profile.
            </p>

            <div class="memory-row">
              <div class="settings-field memory-field">
                <label class="settings-label">
                  Min Heap (-Xms)
                  <span class="settings-hint">MB</span>
                </label>
                <input
                  type="number"
                  class="settings-input"
                  min="128"
                  max="65536"
                  step="128"
                  value={minMemory()}
                  onInput={(e) => setMinMemory(Number(e.currentTarget.value))}
                />
              </div>
              <div class="settings-field memory-field">
                <label class="settings-label">
                  Max Heap (-Xmx)
                  <span class="settings-hint">MB</span>
                </label>
                <input
                  type="number"
                  class="settings-input"
                  min="256"
                  max="65536"
                  step="256"
                  value={maxMemory()}
                  onInput={(e) => setMaxMemory(Number(e.currentTarget.value))}
                />
              </div>
            </div>
          </div>

          {/* ── Save ── */}
          <div class="settings-actions">
            <button
              class="settings-save-btn"
              onClick={handleSave}
              disabled={saving()}
            >
              {saving() ? "Saving..." : "Save Settings"}
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Settings;
