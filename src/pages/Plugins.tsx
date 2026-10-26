/**
 * Plugins (ui-design 09): master-detail with diagnostics banner, install
 * (picker + drag-drop .rmcpkg), tabs (overview / permissions / config /
 * contributions / logic), JSON-Schema-driven config form.
 */
import type { JSX } from "solid-js";
import { Show, For, createMemo, createSignal, onMount, onCleanup, Switch, Match } from "solid-js";
import { PageHeader } from "../components/PageHeader";
import { RmButton, RmIconButton, RmInput, RmSelect, RmSwitch, RmTag } from "../components/rm/controls";
import { RmDialog, RmMenu } from "../components/rm/overlays";
import { RmCard, RmListRow, RmTabs, RmEmptyState, RmErrorState, RmAdvancedSection, RmContentState } from "../components/rm/surfaces";
import { plugins as pluginsApi, pluginUi, logic } from "../ipc/api";
import type { PluginSummary, PluginDetail, CapabilityInfo, InstallReport } from "../ipc/types";
import { pluginsStore } from "../stores/plugins";
import { toast } from "../stores/ui";
import { t } from "../i18n";
import type { TranslationKey } from "../i18n/locales/messages";
import type { PluginContributions } from "../ipc/types";
import { useNavigate } from "@solidjs/router";

export default function Plugins(): JSX.Element {
  const navigate = useNavigate();
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [detail, setDetail] = createSignal<PluginDetail | null>(null);
  const [capabilities, setCapabilities] = createSignal<CapabilityInfo[]>([]);
  const [detailTab, setDetailTab] = createSignal("overview");
  const [ignoredPending, setIgnoredPending] = createSignal<Set<string>>(loadIgnored());
  const [dragOver, setDragOver] = createSignal(false);
  // Remove dialog: the "also delete data" choice lives here (P2-15).
  const [removeTarget, setRemoveTarget] = createSignal<PluginSummary | null>(null);
  const [removeData, setRemoveData] = createSignal(false);

  onMount(async () => {
    await pluginsStore.refresh();
    const caps = await pluginsApi.capabilities();
    if (caps.ok) setCapabilities(caps.data);
    const first = pluginsStore.list()?.plugins[0];
    if (first) void select(first.id);
  });

  async function select(id: string): Promise<void> {
    setSelectedId(id);
    setDetailTab("overview");
    const res = await pluginsApi.inspect(id);
    if (res.ok) setDetail(res.data);
    else toast({ message: res.error.message, tone: "error" });
  }

  function loadIgnored(): Set<string> {
    try {
      return new Set(JSON.parse(localStorage.getItem("rmccc.ui.plugins.ignored") ?? "[]") as string[]);
    } catch {
      return new Set();
    }
  }

  function ignorePending(key: string): void {
    const next = new Set(ignoredPending());
    next.add(key);
    setIgnoredPending(next);
    localStorage.setItem("rmccc.ui.plugins.ignored", JSON.stringify([...next]));
  }

  async function installPicked(): Promise<void> {
    const res = await pluginsApi.pickAndInstall();
    if (res.ok && res.data) handleInstallReport(res.data);
    else if (!res.ok) toast({ message: res.error.message, tone: "error" });
  }

  async function installPath(path: string): Promise<void> {
    const res = await pluginsApi.install(path);
    if (res.ok) handleInstallReport(res.data);
    else toast({ message: res.error.message, tone: "error" });
  }

  function handleInstallReport(report: InstallReport): void {
    void pluginsStore.refresh();
    if (report.requiresReview) {
      toast({ message: t("plugins.install.requiresReview"), tone: "info" });
      void select(report.id);
      setDetailTab("permissions");
    } else {
      toast({ message: t("plugins.installed"), tone: "success", actionLabel: t("common.view"), onAction: () => void select(report.id) });
    }
  }

  async function toggleEnabled(summary: PluginSummary): Promise<void> {
    const res =
      summary.state === "enabled"
        ? await pluginsApi.disable(summary.id)
        : await pluginsApi.enable(summary.id);
    if (res.ok) {
      await pluginsStore.refresh();
      await select(summary.id);
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function remove(summary: PluginSummary, deleteData: boolean): Promise<void> {
    const res = await pluginsApi.remove(summary.id, deleteData);
    if (res.ok) {
      setDetail(null);
      setSelectedId(null);
      await pluginsStore.refresh();
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  async function rollback(summary: PluginSummary): Promise<void> {
    const res = await pluginsApi.rollback(summary.id);
    if (res.ok) {
      toast({ message: t("plugins.rollback.done"), tone: "success" });
      await pluginsStore.refresh();
      await select(summary.id);
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  }

  const list = createMemo(() => pluginsStore.list()?.plugins ?? []);
  const diagnostics = createMemo(() => pluginsStore.list()?.diagnostics ?? []);
  const hasLogic = createMemo(() => (detail()?.manifest.entry?.logic ?? null) !== null);

  return (
    <div class="page">
      <PageHeader
        title={t("plugins.title")}
        actions={
          <div class="btn-row">
            <RmButton size="sm" icon="plus" variant="primary" onClick={() => void installPicked()}>
              {t("plugins.install")}
            </RmButton>
            <RmIconButton icon="refresh" label={t("common.refresh")} onClick={() => void pluginsStore.refresh()} />
          </div>
        }
      />
      <div
        class="page-body page-master-detail"
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          const file = (e.dataTransfer as unknown as DataTransfer).files[0];
          const path = (file as unknown as { path?: string })?.path;
          if (file && file.name.endsWith(".rmcpkg") && path) void installPath(path);
          else if (file) toast({ message: t("plugins.install.dropHint"), tone: "info" });
        }}
      >
        <Show when={dragOver()}>
          <div class="drop-overlay">{t("plugins.install.dropHint")}</div>
        </Show>

        <div class="master-list">
          <Show when={diagnostics().length > 0}>
            <div class="diagnostics-banner">
              ⚠ {t("plugins.diagnostics", { n: diagnostics().length })}
            </div>
          </Show>
          <RmContentState<NonNullable<ReturnType<typeof pluginsStore.list>>>
            data={pluginsStore.list() ?? undefined}
            loading={pluginsStore.loading() && !pluginsStore.list()}
            error={pluginsStore.error()}
            isEmpty={(l) => l.plugins.length === 0}
            empty={
              <RmEmptyState
                icon="extension"
                title={t("plugins.empty.title")}
                hint={t("plugins.empty.hint")}
                action={
                  <RmButton variant="primary" icon="plus" onClick={() => void installPicked()}>
                    {t("plugins.install")}
                  </RmButton>
                }
              />
            }
          >
            {(listData) => (
              <div class="rm-list">
                <For each={listData.plugins}>
                  {(summary) => (
                    <RmListRow
                      selected={selectedId() === summary.id}
                      onClick={() => void select(summary.id)}
                      title={summary.name}
                      subtitle={`v${summary.version}`}
                      trailing={
                        <div class="btn-row">
                          <Show when={summary.lastError}>
                            <span class="dot-error" />
                          </Show>
                          <RmTag tone={summary.state === "enabled" ? "success" : "neutral"}>
                            {t(`plugins.state.${summary.state}` as TranslationKey)}
                          </RmTag>
                        </div>
                      }
                    />
                  )}
                </For>
              </div>
            )}
          </RmContentState>
        </div>

        <div class="detail-pane">
          {/* keyed：d 是当次渲染的快照值；非 keyed 访问器在 detail 重置后于事件
              回调里读取会抛 "Stale read from <Show>"，错误风暴会卡死 WebView。 */}
          <Show when={detail()} keyed fallback={<RmEmptyState icon="extension" title={t("plugins.empty.select")} />}>
            {(d) => (
              <div class="plugin-detail">
                <div class="plugin-detail-head">
                  <h3>{d.summary.name}</h3>
                  <RmTag>v{d.summary.version}</RmTag>
                  <RmTag tone={d.summary.state === "enabled" ? "success" : "neutral"}>
                    {t(`plugins.state.${d.summary.state}` as TranslationKey)}
                  </RmTag>
                  <div class="flex-spacer" />
                  <RmButton
                    size="sm"
                    variant={d.summary.state === "enabled" ? "secondary" : "primary"}
                    onClick={() => void toggleEnabled(d.summary)}
                  >
                    {d.summary.state === "enabled" ? t("plugins.disable") : t("plugins.enable")}
                  </RmButton>
                  <RmMenu
                    align="right"
                    items={[
                      ...(d.summary.previousVersion
                        ? [{
                            label: t("plugins.rollback", { version: d.summary.previousVersion }),
                            icon: "refresh" as const,
                            onSelect: () => void rollback(d.summary),
                          }]
                        : []),
                      {
                        label: t("plugins.remove.title"),
                        icon: "trash" as const,
                        danger: true,
                        onSelect: () => {
                          setRemoveData(false);
                          setRemoveTarget(d.summary);
                        },
                      },
                    ]}
                  >
                    <RmIconButton icon="more-horizontal" label={t("common.more")} />
                  </RmMenu>
                </div>

                <RmTabs
                  items={[
                    { id: "overview", label: t("plugins.detail.tab.overview") },
                    { id: "permissions", label: t("plugins.detail.tab.permissions") },
                    { id: "config", label: t("plugins.detail.tab.config") },
                    { id: "contributions", label: t("plugins.detail.tab.contributions") },
                    ...(hasLogic() ? [{ id: "logic", label: t("plugins.detail.tab.logic") }] : []),
                  ]}
                  active={detailTab()}
                  onSelect={setDetailTab}
                />

                <Switch>
                  <Match when={detailTab() === "overview"}>
                    <OverviewSection detail={d} />
                  </Match>
                  <Match when={detailTab() === "permissions"}>
                    <PermissionsSection detail={d} capabilities={capabilities()} ignored={ignoredPending()} onIgnore={ignorePending} onChanged={() => void select(d.summary.id)} />
                  </Match>
                  <Match when={detailTab() === "config"}>
                    <ConfigSection pluginId={d.summary.id} hasSchema={d.hasConfigSchema} />
                  </Match>
                  <Match when={detailTab() === "contributions"}>
                    <ContributionsSection pluginId={d.summary.id} />
                  </Match>
                  <Match when={detailTab() === "logic"}>
                    <LogicSection pluginId={d.summary.id} />
                  </Match>
                </Switch>
              </div>
            )}
          </Show>
        </div>
      </div>

      <RmDialog
        open={removeTarget() !== null}
        title={t("plugins.remove.title")}
        alertRole
        onClose={() => setRemoveTarget(null)}
        footer={
          <>
            <RmButton variant="ghost" onClick={() => setRemoveTarget(null)}>
              {t("common.cancel")}
            </RmButton>
            <RmButton
              variant="danger"
              icon="trash"
              onClick={() => {
                const target = removeTarget();
                if (target) void remove(target, removeData());
              }}
            >
              {t("plugins.remove.confirm")}
            </RmButton>
          </>
        }
      >
        <div class="rm-confirm-body">
          {t("plugins.remove.body", { name: removeTarget()?.name ?? "" })}
        </div>
        <div class="path-row">
          <span class="path-row-value">{t("plugins.remove.deleteData")}</span>
          <RmSwitch
            checked={removeData()}
            onChange={setRemoveData}
            label={t("plugins.remove.deleteData")}
          />
        </div>
      </RmDialog>
    </div>
  );
}

function OverviewSection(props: { detail: PluginDetail }): JSX.Element {
  return (
    <RmCard title={t("plugins.overview.info")}>
      <div class="kv-grid">
        <span>{t("plugins.overview.publisher")}</span><span>{props.detail.summary.publisher.name}</span>
        <span>{t("plugins.overview.description")}</span><span>{props.detail.summary.description ?? "—"}</span>
        <span>{t("plugins.overview.path")}</span><span class="rm-input-mono">{props.detail.installedPath}</span>
        <span>{t("plugins.overview.installedAt")}</span><span>{props.detail.summary.installedAt}</span>
        <Show when={props.detail.summary.previousVersion}>
          <span>{t("plugins.overview.previousVersion")}</span><span>v{props.detail.summary.previousVersion}</span>
        </Show>
      </div>
      <Show when={props.detail.summary.lastError}>
        <div class="warning-note">{props.detail.summary.lastError}</div>
      </Show>
    </RmCard>
  );
}

function PermissionsSection(props: {
  detail: PluginDetail;
  capabilities: CapabilityInfo[];
  ignored: Set<string>;
  onIgnore: (key: string) => void;
  onChanged: () => void;
}): JSX.Element {
  const pending = createMemo(() => props.detail.pending.filter((p) => !props.ignored.has(`${props.detail.summary.id}:${p}`)));
  const riskOf = (name: string) => props.capabilities.find((c) => c.name === name)?.risk ?? "low";

  async function grant(capability: string): Promise<void> {
    const res = await pluginsApi.grant(props.detail.summary.id, capability);
    if (res.ok) {
      toast({ message: t("plugins.perms.granted"), tone: "success" });
      props.onChanged();
    } else toast({ message: res.error.message, tone: "error" });
  }

  async function revoke(capability: string): Promise<void> {
    const res = await pluginsApi.revoke(props.detail.summary.id, capability);
    if (res.ok) {
      toast({ message: t("plugins.perms.revoked"), tone: "success" });
      props.onChanged();
    } else toast({ message: res.error.message, tone: "error" });
  }

  return (
    <div class="tab-stack">
      <Show when={pending().length > 0}>
        <RmCard title={t("plugins.perms.pending")} danger>
          <For each={pending()}>
            {(capability) => (
              <RmListRow
                title={capability}
                subtitle={t("plugins.perms.risk", { risk: riskOf(capability) })}
                trailing={
                  <div class="btn-row">
                    <RmButton size="sm" variant="primary" onClick={() => void grant(capability)}>
                      {t("plugins.perms.grant")}
                    </RmButton>
                    <RmButton size="sm" variant="ghost" onClick={() => props.onIgnore(`${props.detail.summary.id}:${capability}`)}>
                      {t("plugins.perms.ignore")}
                    </RmButton>
                  </div>
                }
              />
            )}
          </For>
        </RmCard>
      </Show>
      <RmCard title={t("plugins.perms.grantedTitle")}>
        <For each={props.detail.granted}>
          {(grantRow) => (
            <RmListRow
              title={`✓ ${grantRow.capability}`}
              subtitle={t("plugins.perms.grantedAt", { time: grantRow.grantedAt })}
              trailing={
                <RmButton size="sm" variant="ghost" onClick={() => void revoke(grantRow.capability)}>
                  {t("plugins.perms.revoke")}
                </RmButton>
              }
            />
          )}
        </For>
        <Show when={props.detail.granted.length === 0}>
          <div class="muted-line">{t("plugins.perms.none")}</div>
        </Show>
      </RmCard>
      <RmAdvancedSection title={t("plugins.perms.catalog")}>
        <For each={props.capabilities}>
          {(capability) => (
            <RmListRow
              title={capability.name}
              subtitle={capability.description}
              trailing={<RmTag tone={capability.risk === "high" ? "danger" : capability.risk === "medium" ? "warning" : "success"}>{capability.risk}</RmTag>}
            />
          )}
        </For>
      </RmAdvancedSection>
    </div>
  );
}

function ConfigSection(props: { pluginId: string; hasSchema: boolean }): JSX.Element {
  const [valuesText, setValuesText] = createSignal("{}");
  const [schema, setSchema] = createSignal<Record<string, unknown> | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  onMount(async () => {
    const res = await pluginsApi.configGet(props.pluginId);
    if (res.ok) {
      setValuesText(JSON.stringify((res.data.config as { values?: unknown }).values ?? {}, null, 2));
      setSchema((res.data.schema as Record<string, unknown>) ?? null);
    } else {
      toast({ message: res.error.message, tone: "error" });
    }
  });

  async function save(): Promise<void> {
    try {
      const parsed = JSON.parse(valuesText());
      const res = await pluginsApi.configSet(props.pluginId, parsed);
      if (res.ok) toast({ message: t("plugins.config.saved"), tone: "success" });
      else setError(res.error.message);
    } catch (e) {
      setError(t("plugins.config.invalid"));
    }
  }

  return (
    <RmCard title={t("plugins.config.title")}>
      <Show
        when={props.hasSchema && schema()}
        fallback={<div class="muted-line">{t("plugins.config.noSchema")}</div>}
      >
        {/* Schema-driven form: v1 renders known scalar types; complex schemas
            fall back to the JSON editor below. */}
        <SchemaForm schema={schema()!} valuesText={valuesText()} setValuesText={setValuesText} />
      </Show>
      <RmAdvancedSection title={t("plugins.config.rawJson")}>
        <textarea class="rm-textarea rm-input-mono" rows={10} value={valuesText()} onInput={(e) => setValuesText(e.currentTarget.value)} />
      </RmAdvancedSection>
      <Show when={error()}>
        <div class="rm-field-error">{error()}</div>
      </Show>
      <div class="btn-row">
        <RmButton variant="primary" onClick={() => void save()}>{t("common.save")}</RmButton>
        <RmButton
          onClick={async () => {
            const res = await pluginsApi.configReset(props.pluginId);
            if (res.ok) {
              setValuesText(JSON.stringify(res.data.values, null, 2));
              toast({ message: t("plugins.config.reset"), tone: "info" });
            }
          }}
        >
          {t("plugins.config.reset")}
        </RmButton>
      </div>
    </RmCard>
  );
}

/** Minimal schema form: string / number / boolean at top level. */
function SchemaForm(props: {
  schema: Record<string, unknown>;
  valuesText: string;
  setValuesText: (text: string) => void;
}): JSX.Element {
  const values = createMemo<Record<string, unknown>>(() => {
    try {
      return JSON.parse(props.valuesText) as Record<string, unknown>;
    } catch {
      return {};
    }
  });
  const properties = createMemo(
    () => (props.schema.properties as Record<string, Record<string, unknown>> | undefined) ?? {}
  );

  function setField(key: string, value: unknown): void {
    props.setValuesText(JSON.stringify({ ...values(), [key]: value }, null, 2));
  }

  return (
    <div class="schema-form">
      <For each={Object.entries(properties())}>
        {([key, prop]) => (
          <Switch>
            <Match when={prop.enum}>
              <RmListRow
                title={key}
                subtitle={String(prop.description ?? "")}
                trailing={
                  <RmInput
                    value={String(values()[key] ?? prop.default ?? "")}
                    onInput={(v) => setField(key, v)}
                  />
                }
              />
            </Match>
            <Match when={prop.type === "boolean"}>
              <RmListRow
                title={key}
                subtitle={String(prop.description ?? "")}
                trailing={
                  <RmSwitch checked={Boolean(values()[key])} onChange={(v) => setField(key, v)} />
                }
              />
            </Match>
            <Match when={prop.type === "integer" || prop.type === "number"}>
              <RmListRow
                title={key}
                subtitle={String(prop.description ?? "")}
                trailing={
                  <input
                    class="rm-input"
                    style={{ width: "110px" }}
                    type="number"
                    value={Number(values()[key] ?? prop.default ?? 0)}
                    onInput={(e) => setField(key, Number(e.currentTarget.value))}
                  />
                }
              />
            </Match>
            <Match when={true}>
              <RmListRow
                title={key}
                subtitle={String(prop.description ?? "")}
                trailing={
                  <RmInput
                    value={String(values()[key] ?? prop.default ?? "")}
                    onInput={(v) => setField(key, v)}
                  />
                }
              />
            </Match>
          </Switch>
        )}
      </For>
    </div>
  );
}

function ContributionsSection(props: { pluginId: string }): JSX.Element {
  const navigate = useNavigate();
  const [contributions, setContributions] = createSignal<PluginContributions | null>(null);

  onMount(async () => {
    const res = await pluginsApi.contributions();
    if (res.ok) setContributions(res.data);
    else toast({ message: res.error.message, tone: "error" });
  });

  const cards = createMemo(() => (contributions()?.dashboardCards ?? []).filter((c) => c.pluginId === props.pluginId));
  const themes = createMemo(() => (contributions()?.themes ?? []).filter((x) => x.pluginId === props.pluginId));
  const pages = createMemo(() => (contributions()?.settingsPages ?? []).filter((x) => x.pluginId === props.pluginId));

  return (
    <RmCard title={t("plugins.contrib.title")}>
      <div class="muted-line">{t("plugins.contrib.cards", { n: cards().length })} · {t("plugins.contrib.themes", { n: themes().length })} · {t("plugins.contrib.pages", { n: pages().length })}</div>
      <div class="btn-row">
        <RmButton size="sm" icon="home" onClick={() => navigate("/")}>{t("plugins.contrib.goHome")}</RmButton>
        <RmButton size="sm" icon="sliders" onClick={() => navigate("/settings#plugins")}>{t("plugins.contrib.goSettings")}</RmButton>
      </div>
    </RmCard>
  );
}

/** v1 read-only call whitelist (ADR-007). */
const LOGIC_CALLS = ["compat.check", "mods.source.search"];

function LogicSection(props: { pluginId: string }): JSX.Element {
  const [input, setInput] = createSignal("{}");
  const [running, setRunning] = createSignal(false);
  const [result, setResult] = createSignal<string>("");
  const [activeCall, setActiveCall] = createSignal(LOGIC_CALLS[0]);

  async function run(): Promise<void> {
    setRunning(true);
    setResult("");
    try {
      const parsed = JSON.parse(input());
      const res = await logic.invoke(props.pluginId, activeCall(), parsed, 30000);
      setResult(res.ok ? JSON.stringify(res.data, null, 2) : res.error.message);
    } catch {
      setResult(t("plugins.logic.invalidInput"));
    }
    setRunning(false);
  }

  return (
    <RmCard title={t("plugins.logic.title")}>
      <div class="muted-line">{t("plugins.logic.sandboxNote")}</div>
      <RmSelect
        value={activeCall()}
        onChange={setActiveCall}
        options={LOGIC_CALLS.map((call) => ({ value: call, label: call }))}
      />
      <textarea class="rm-textarea rm-input-mono" rows={4} value={input()} onInput={(e) => setInput(e.currentTarget.value)} />
      <div class="btn-row">
        <RmButton variant="primary" icon="play" loading={running()} onClick={() => void run()}>
          {t("plugins.logic.run")}
        </RmButton>
        <RmButton
          icon="square"
          onClick={async () => {
            await logic.cancel(props.pluginId);
          }}
        >
          {t("plugins.logic.cancel")}
        </RmButton>
      </div>
      <Show when={result()}>
        <pre class="logic-result">{result()}</pre>
      </Show>
    </RmCard>
  );
}
