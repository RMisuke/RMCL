/**
 * Plugins page (Phase 2).
 *
 * Declarative plugin manager: install `.rmcpkg`, review permissions, enable /
 * disable / rollback / uninstall, edit schema-validated config. All authority
 * lives in the backend `PluginManager`; this page only renders summaries and
 * forwards explicit user actions.
 */
import { Component, For, Show, createSignal, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { plugins as pluginApi } from "../ipc/api";
import type {
  CapabilityInfo,
  PluginDetail,
  PluginDiagnostic,
  PluginState,
  PluginSummary,
} from "../ipc/types";
import { showToast } from "../components/Toast";
import { t } from "../i18n";

// ── Helpers ──

function notify(message: string) {
  showToast({ code: "ok", message, retryable: false });
}

function stateLabel(state: PluginState): string {
  switch (state) {
    case "enabled":
      return t("plugins.state.enabled");
    case "disabled":
      return t("plugins.state.disabled");
    case "failed":
      return t("plugins.state.failed");
    case "removed":
      return t("plugins.state.removed");
    case "installed_disabled":
      return t("plugins.state.installed_disabled");
    default:
      return state;
  }
}

const riskLabel = (risk: string) =>
  risk === "low"
    ? t("plugins.risk.low")
    : risk === "medium"
      ? t("plugins.risk.medium")
      : t("plugins.risk.high");

const Plugins: Component = () => {
  const navigate = useNavigate();

  const [list, setList] = createSignal<PluginSummary[]>([]);
  const [diagnostics, setDiagnostics] = createSignal<PluginDiagnostic[]>([]);
  const [caps, setCaps] = createSignal<CapabilityInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal<string | null>(null);

  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [detail, setDetail] = createSignal<PluginDetail | null>(null);
  const [configText, setConfigText] = createSignal("");
  const [deleteData, setDeleteData] = createSignal(false);

  async function load() {
    const [listRes, capsRes] = await Promise.all([
      pluginApi.list(),
      pluginApi.capabilities(),
    ]);
    if (listRes.ok) {
      setList(listRes.data.plugins);
      setDiagnostics(listRes.data.diagnostics);
    } else {
      showToast(listRes.error);
    }
    if (capsRes.ok) setCaps(capsRes.data);
    setLoading(false);
  }

  async function openDetail(id: string) {
    const res = await pluginApi.inspect(id);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    setSelectedId(id);
    setDetail(res.data);
    setDeleteData(false);
    setConfigText(JSON.stringify(res.data.config ?? {}, null, 2));
  }

  function closeDetail() {
    setSelectedId(null);
    setDetail(null);
    setConfigText("");
  }

  async function refreshDetail(id: string) {
    if (selectedId() === id) await openDetail(id);
  }

  async function install() {
    setBusy("install");
    const res = await pluginApi.pickAndInstall();
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    if (!res.data) return; // cancelled
    notify(t("plugins.installDone", { id: res.data.id }));
    if (res.data.requiresReview) {
      notify(
        t("plugins.pendingReview", {
          count: res.data.pendingPermissions.length,
        })
      );
    }
    await load();
    await openDetail(res.data.id);
  }

  async function setEnabled(plugin: PluginSummary, enabled: boolean) {
    setBusy(plugin.id);
    const res = enabled
      ? await pluginApi.enable(plugin.id)
      : await pluginApi.disable(plugin.id);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(enabled ? t("plugins.enabled") : t("plugins.disabled"));
    await load();
    await refreshDetail(plugin.id);
  }

  async function remove(plugin: PluginSummary, withData: boolean) {
    if (!window.confirm(t("plugins.removeConfirm", { name: plugin.name }))) {
      return;
    }
    setBusy(plugin.id);
    const res = await pluginApi.remove(plugin.id, withData);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.removed"));
    if (selectedId() === plugin.id) closeDetail();
    await load();
  }

  async function rollback(plugin: PluginSummary) {
    setBusy(plugin.id);
    const res = await pluginApi.rollback(plugin.id);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.rolledBack"));
    await load();
    await refreshDetail(plugin.id);
  }

  async function grant(pluginId: string, capability: string) {
    setBusy(pluginId);
    const res = await pluginApi.grant(pluginId, capability);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.permissionGranted"));
    await load();
    await refreshDetail(pluginId);
  }

  async function revoke(pluginId: string, capability: string) {
    setBusy(pluginId);
    const res = await pluginApi.revoke(pluginId, capability);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.permissionRevoked"));
    await load();
    await refreshDetail(pluginId);
  }

  async function saveConfig(pluginId: string) {
    let values: unknown;
    try {
      values = JSON.parse(configText());
    } catch {
      showToast({
        code: "validation_error",
        message: t("plugins.config.invalidJson"),
        retryable: false,
      });
      return;
    }
    setBusy(pluginId);
    const res = await pluginApi.configSet(pluginId, values);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.config.saved"));
    await refreshDetail(pluginId);
  }

  async function resetConfig(pluginId: string) {
    if (!window.confirm(t("plugins.config.resetConfirm"))) return;
    setBusy(pluginId);
    const res = await pluginApi.configReset(pluginId);
    setBusy(null);
    if (!res.ok) {
      showToast(res.error);
      return;
    }
    notify(t("plugins.config.saved"));
    await refreshDetail(pluginId);
  }

  const capabilityInfo = (name: string) =>
    caps().find((c) => c.name === name);

  onMount(load);

  return (
    <div class="settings-page plugins-page">
      <div class="settings-header">
        <button class="settings-back-btn" onClick={() => navigate("/")}>
          {t("common.back")}
        </button>
        <h2 class="settings-title">{t("plugins.title")}</h2>
        <div style="flex:1" />
        <button
          class="settings-refresh-btn"
          onClick={install}
          disabled={busy() !== null}
        >
          {busy() === "install" ? t("plugins.installing") : t("plugins.install")}
        </button>
      </div>

      <Show
        when={!loading()}
        fallback={
          <div class="loading-inline">
            <div class="loading-spinner-small" />
            <span>{t("plugins.loading")}</span>
          </div>
        }
      >
        <div class="settings-content">
          <p class="settings-hint plugins-unsigned-hint">
            ⚠ {t("plugins.unsignedHint")}
          </p>

          <Show when={diagnostics().length > 0}>
            <div class="settings-section plugin-diagnostics">
              <h3 class="settings-section-title">
                {t("plugins.diagnostics.title")}
              </h3>
              <For each={diagnostics()}>
                {(d) => (
                  <div
                    class={`plugin-diagnostic ${d.severity === "error" ? "is-error" : "is-warning"}`}
                  >
                    <span class="plugin-diagnostic-id">
                      {d.pluginId ?? "—"}
                    </span>
                    <span>{d.message}</span>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show
            when={list().length > 0}
            fallback={
              <div class="settings-section plugins-empty">
                <h3 class="settings-section-title">{t("plugins.empty")}</h3>
                <p class="settings-section-desc">{t("plugins.emptyHint")}</p>
                <button
                  class="settings-refresh-btn"
                  onClick={install}
                  disabled={busy() !== null}
                >
                  {t("plugins.install")}
                </button>
              </div>
            }
          >
            <div class="plugin-list">
              <For each={list()}>
                {(plugin) => (
                  <div
                    class={`plugin-card ${selectedId() === plugin.id ? "is-selected" : ""}`}
                  >
                    <div class="plugin-card-head">
                      <div class="plugin-card-title">
                        <span class="plugin-name">{plugin.name}</span>
                        <span class="plugin-version">
                          {t("plugins.version", { version: plugin.version })}
                        </span>
                        <span class={`plugin-badge state-${plugin.state}`}>
                          {stateLabel(plugin.state)}
                        </span>
                        <Show when={plugin.pendingPermissions.length > 0}>
                          <span class="plugin-badge state-pending">
                            {t("plugins.permissions.pending")}
                          </span>
                        </Show>
                      </div>
                      <div class="plugin-card-actions">
                        <Show
                          when={plugin.state === "enabled"}
                          fallback={
                            <button
                              class="modal-btn-primary plugin-btn"
                              disabled={busy() === plugin.id}
                              onClick={() => setEnabled(plugin, true)}
                            >
                              {t("plugins.enable")}
                            </button>
                          }
                        >
                          <button
                            class="modal-btn-cancel plugin-btn"
                            disabled={busy() === plugin.id}
                            onClick={() => setEnabled(plugin, false)}
                          >
                            {t("plugins.disable")}
                          </button>
                        </Show>
                        <Show when={plugin.previousVersion}>
                          <button
                            class="modal-btn-cancel plugin-btn"
                            disabled={busy() === plugin.id}
                            onClick={() => rollback(plugin)}
                          >
                            {t("plugins.rollback")}
                          </button>
                        </Show>
                        <button
                          class="modal-btn-cancel plugin-btn"
                          disabled={busy() === plugin.id}
                          onClick={() =>
                            selectedId() === plugin.id
                              ? closeDetail()
                              : openDetail(plugin.id)
                          }
                        >
                          {selectedId() === plugin.id
                            ? t("plugins.hideDetails")
                            : t("plugins.details")}
                        </button>
                        <button
                          class="modal-btn-cancel plugin-btn plugin-btn-danger"
                          disabled={busy() === plugin.id}
                          onClick={() => remove(plugin, false)}
                        >
                          {t("plugins.remove")}
                        </button>
                      </div>
                    </div>

                    <p class="plugin-description">
                      {plugin.description ?? "—"}
                    </p>

                    <div class="plugin-meta">
                      <span class="plugin-meta-item">
                        {plugin.publisher.name}
                      </span>
                      <span class="plugin-meta-item plugin-mono">
                        {plugin.id}
                      </span>
                      <span class="plugin-meta-item">
                        {t("plugins.trust.development")}
                      </span>
                      <span class="plugin-meta-item">
                        {t("plugins.contributions.summary", {
                          cards: plugin.dashboardCards,
                          pages: plugin.settingsPages,
                          themes: plugin.themes,
                        })}
                      </span>
                    </div>

                    <Show when={selectedId() === plugin.id && detail()}>
                      {(d) => (
                        <div class="plugin-detail">
                          <div class="plugin-detail-grid">
                            <div>
                              <h4 class="plugin-detail-title">
                                {t("plugins.permissions.title")}
                              </h4>
                              <Show
                                when={d().manifest.permissions.length > 0}
                                fallback={
                                  <p class="settings-hint">
                                    {t("plugins.permissions.none")}
                                  </p>
                                }
                              >
                                <For each={d().manifest.permissions}>
                                  {(perm) => {
                                    const granted = () =>
                                      d().summary.grantedPermissions.includes(
                                        perm.capability
                                      );
                                    const pending = () =>
                                      d().summary.pendingPermissions.includes(
                                        perm.capability
                                      );
                                    const info = capabilityInfo(
                                      perm.capability
                                    );
                                    return (
                                      <div class="plugin-perm-row">
                                        <div class="plugin-perm-main">
                                          <code>{perm.capability}</code>
                                          <span
                                            class={`plugin-risk risk-${info?.risk ?? "high"}`}
                                          >
                                            {riskLabel(
                                              info?.risk ?? "high"
                                            )}
                                          </span>
                                          <Show
                                            when={
                                              perm.hosts.length > 0
                                            }
                                          >
                                            <span class="plugin-scope">
                                              {perm.hosts.join(", ")}
                                            </span>
                                          </Show>
                                          <Show when={perm.reason}>
                                            <span class="plugin-perm-reason">
                                              {perm.reason}
                                            </span>
                                          </Show>
                                        </div>
                                        <div class="plugin-perm-actions">
                                          <span
                                            class={`plugin-grant-state ${granted() ? "is-granted" : pending() ? "is-pending" : ""}`}
                                          >
                                            {granted()
                                              ? t(
                                                  "plugins.permissions.granted"
                                                )
                                              : pending()
                                                ? t(
                                                    "plugins.permissions.pending"
                                                  )
                                                : "—"}
                                          </span>
                                          <Show
                                            when={!granted()}
                                            fallback={
                                              <button
                                                class="modal-btn-cancel plugin-btn"
                                                disabled={
                                                  busy() === plugin.id
                                                }
                                                onClick={() =>
                                                  revoke(
                                                    plugin.id,
                                                    perm.capability
                                                  )
                                                }
                                              >
                                                {t(
                                                  "plugins.permissions.revoke"
                                                )}
                                              </button>
                                            }
                                          >
                                            <button
                                              class="modal-btn-primary plugin-btn"
                                              disabled={
                                                busy() === plugin.id
                                              }
                                              onClick={() =>
                                                grant(
                                                  plugin.id,
                                                  perm.capability
                                                )
                                              }
                                            >
                                              {t(
                                                "plugins.permissions.grant"
                                              )}
                                            </button>
                                          </Show>
                                        </div>
                                      </div>
                                    );
                                  }}
                                </For>
                              </Show>
                              <Show
                                when={d().summary.pendingPermissions.length >
                                  0}
                              >
                                <p class="plugin-review-hint">
                                  {t("plugins.permissions.reviewHint")}
                                </p>
                              </Show>
                            </div>

                            <div>
                              <h4 class="plugin-detail-title">
                                {t("plugins.deps.title")}
                              </h4>
                              <Show
                                when={d().manifest.extensionDependencies.length > 0}
                                fallback={
                                  <p class="settings-hint">
                                    {t("plugins.deps.none")}
                                  </p>
                                }
                              >
                                <For
                                  each={d().manifest.extensionDependencies}
                                >
                                  {(dep) => {
                                    const status = () =>
                                      d().dependencies.find(
                                        (s) => s.id === dep.id
                                      );
                                    return (
                                      <div class="plugin-dep-row">
                                        <code>{dep.id}</code>
                                        <span class="plugin-scope">
                                          {dep.version}
                                        </span>
                                        <Show when={dep.optional}>
                                          <span class="plugin-badge">
                                            {t("plugins.deps.optional")}
                                          </span>
                                        </Show>
                                        <span
                                          class={`plugin-dep-state ${status()?.satisfied ? "is-ok" : "is-bad"}`}
                                        >
                                          {status()?.satisfied
                                            ? t("plugins.deps.satisfied")
                                            : t("plugins.deps.missing")}
                                        </span>
                                      </div>
                                    );
                                  }}
                                </For>
                              </Show>
                              <Show when={d().dependents.length > 0}>
                                <p class="settings-hint">
                                  {t("plugins.requiredBy")}:{" "}
                                  {d().dependents.join(", ")}
                                </p>
                              </Show>

                              <h4 class="plugin-detail-title">
                                {t("plugins.config.title")}
                              </h4>
                              <Show
                                when={d().hasConfigSchema}
                                fallback={
                                  <p class="settings-hint">
                                    {t("plugins.config.noSchema")}
                                  </p>
                                }
                              >
                                <p class="settings-hint">
                                  {t("plugins.config.desc")}
                                </p>
                                <textarea
                                  class="settings-input settings-input-mono plugin-config-editor"
                                  rows="8"
                                  value={configText()}
                                  onInput={(e) =>
                                    setConfigText(e.currentTarget.value)
                                  }
                                />
                                <div class="plugin-config-actions">
                                  <button
                                    class="modal-btn-primary plugin-btn"
                                    disabled={busy() === plugin.id}
                                    onClick={() => saveConfig(plugin.id)}
                                  >
                                    {t("plugins.config.save")}
                                  </button>
                                  <button
                                    class="modal-btn-cancel plugin-btn"
                                    disabled={busy() === plugin.id}
                                    onClick={() => resetConfig(plugin.id)}
                                  >
                                    {t("plugins.config.reset")}
                                  </button>
                                </div>
                              </Show>

                              <p class="plugin-path">
                                {d().installedPath}
                              </p>

                              <label class="plugin-delete-data">
                                <input
                                  type="checkbox"
                                  checked={deleteData()}
                                  onChange={(e) =>
                                    setDeleteData(e.currentTarget.checked)
                                  }
                                />
                                {t("plugins.removeDeleteData")}
                              </label>
                              <button
                                class="modal-btn-cancel plugin-btn plugin-btn-danger"
                                disabled={busy() === plugin.id}
                                onClick={() => remove(plugin, deleteData())}
                              >
                                {t("plugins.remove")}
                              </button>
                            </div>
                          </div>
                        </div>
                      )}
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default Plugins;
