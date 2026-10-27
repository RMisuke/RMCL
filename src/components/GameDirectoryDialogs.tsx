/**
 * Game-directory management, HMCL/PCL style: one drawer lists every registered
 * root (rename / reveal / set default / remove), and a scanner imports roots
 * already known to other launchers instead of asking the user to re-add them.
 */
import type { JSX } from "solid-js";
import { Show, For, createSignal, createEffect, on } from "solid-js";
import { Icon } from "./icons";
import { RmButton, RmCheckbox, RmIconButton, RmTag } from "./rm/controls";
import { RmDialog } from "./rm/overlays";
import { RmEmptyState, RmErrorState } from "./rm/surfaces";
import { instancesStore } from "../stores/instances";
import { uiStore, toast, promptText } from "../stores/ui";
import { t } from "../i18n";
import { elidePath } from "../lib/format";
import type { DirectoryCandidate, DirectorySource, GameDirectoryOverview } from "../ipc/types";

const SOURCE_LABEL: Record<DirectorySource, string> = {
  rmccc: "RMCCC",
  mojang: "官方",
  hmcl: "HMCL",
  pcl: "PCL",
  manual: "手动",
};

function sourceLabel(source: DirectorySource): string {
  return SOURCE_LABEL[source] ?? source;
}

function layoutLabel(entry: GameDirectoryOverview): string {
  if (entry.layout.standard) return t("instances.dir.standard");
  if (!entry.layout.has_versions) return t("instances.dir.empty");
  return t("instances.dir.notStandard");
}

// ── Manage registered directories ──────────────────────────────────

export function DirectoryManageDialog(props: {
  open: boolean;
  onClose: () => void;
  onAdd: () => void;
  onScan: () => void;
}): JSX.Element {
  const entries = () => instancesStore.directories() ?? [];
  const [busy, setBusy] = createSignal<string | null>(null);

  async function setDefault(entry: GameDirectoryOverview): Promise<void> {
    if (entry.is_selected) return;
    setBusy(entry.directory.id);
    const res = await instancesStore.selectDirectory(entry.directory.id);
    setBusy(null);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
    else toast({ message: t("instances.dir.defaultSet", { name: entry.directory.name }), tone: "success" });
  }

  async function rename(entry: GameDirectoryOverview): Promise<void> {
    const next = await promptText({
      title: t("instances.dir.rename"),
      label: t("instances.dir.renameHint"),
      initial: entry.directory.name,
    });
    if (!next || next === entry.directory.name) return;
    const res = await instancesStore.updateDirectory(entry.directory.id, next);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
    else toast({ message: t("instances.dir.renamed"), tone: "success" });
  }

  async function remove(entry: GameDirectoryOverview): Promise<void> {
    const allowed = await uiStore.confirm({
      title: t("instances.root.remove.title"),
      body: t("instances.root.remove.body", { path: entry.directory.path }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!allowed) return;
    const res = await instancesStore.removeDirectory(entry.directory.id);
    if (!res.ok) toast({ message: res.error.message, tone: "error" });
    else toast({ message: t("instances.dir.removed"), tone: "success" });
  }

  return (
    <RmDialog
      open={props.open}
      title={t("instances.manage.title")}
      width={680}
      onClose={props.onClose}
      footer={
        <>
          <RmButton onClick={props.onClose}>{t("common.close")}</RmButton>
          <RmButton variant="primary" icon="search" onClick={props.onScan}>
            {t("instances.manage.scan")}
          </RmButton>
        </>
      }
    >
      <div class="dir-manage">
        <div class="dir-manage-hint">{t("instances.manage.hint")}</div>
        <div class="dir-manage-toolbar">
          <RmButton size="sm" icon="plus" onClick={props.onAdd}>
            {t("instances.sidebar.add")}
          </RmButton>
          <RmButton size="sm" icon="search" onClick={props.onScan}>
            {t("instances.manage.scan")}
          </RmButton>
        </div>
        <div class="dir-manage-list">
          <For each={entries()}>
            {(entry) => (
              <div
                class={`dir-manage-row${entry.is_selected ? " is-active" : ""}${
                  busy() === entry.directory.id ? " is-busy" : ""
                }`}
              >
                <button
                  class={`dir-manage-default${entry.is_selected ? " is-on" : ""}`}
                  title={t("instances.dir.setDefault")}
                  aria-pressed={entry.is_selected}
                  onClick={() => void setDefault(entry)}
                >
                  <Icon name={entry.is_selected ? "check" : "folder"} size={16} />
                </button>
                <button class="dir-manage-main" onClick={() => void setDefault(entry)}>
                  <div class="dir-manage-title">
                    <span class="dir-manage-name">{entry.directory.name}</span>
                    <Show when={entry.directory.kind === "external"}>
                      <RmTag tone="success">{t("instances.root.external")}</RmTag>
                    </Show>
                    <Show when={entry.is_selected}>
                      <RmTag tone="accent">{t("instances.dir.default")}</RmTag>
                    </Show>
                    <Show when={entry.layout.has_hmcl_metadata}>
                      <RmTag>HMCL</RmTag>
                    </Show>
                    <Show when={entry.layout.has_pcl_metadata}>
                      <RmTag>PCL</RmTag>
                    </Show>
                  </div>
                  <div class="dir-manage-path" title={entry.directory.path}>
                    {elidePath(entry.directory.path, 76)}
                  </div>
                  <div class="dir-manage-meta">
                    {t("instances.manage.instanceCount", { count: entry.instance_count })} ·{" "}
                    {layoutLabel(entry)}
                  </div>
                </button>
                <div class="dir-manage-actions">
                  <RmIconButton
                    icon="edit"
                    label={t("instances.dir.rename")}
                    onClick={() => void rename(entry)}
                  />
                  <RmIconButton
                    icon="folder"
                    label={t("instances.dir.reveal")}
                    onClick={() => void instancesStore.revealDirectory(entry.directory.id)}
                  />
                  <RmIconButton
                    icon="trash"
                    danger
                    label={t("instances.root.remove.title")}
                    disabled={entries().length <= 1}
                    onClick={() => void remove(entry)}
                  />
                </div>
              </div>
            )}
          </For>
        </div>
      </div>
    </RmDialog>
  );
}

// ── Scan other launchers ───────────────────────────────────────────

export function DirectoryScanDialog(props: {
  open: boolean;
  onClose: () => void;
}): JSX.Element {
  const [candidates, setCandidates] = createSignal<DirectoryCandidate[] | undefined>(undefined);
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  /** Nickname per candidate path; only the RMCCC display name, never the folder. */
  const [names, setNames] = createSignal<Map<string, string>>(new Map());
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [importing, setImporting] = createSignal(false);

  createEffect(
    on(
      () => props.open,
      (open) => {
        if (open) void scan();
      }
    )
  );

  async function scan(): Promise<void> {
    setLoading(true);
    setError(null);
    const res = await instancesStore.scanDirectories();
    setLoading(false);
    if (!res.ok) {
      setError(res.error.message);
      return;
    }
    setCandidates(res.data);
    setSelected(
      new Set(
        res.data
          .filter((entry) => !entry.registered && entry.layout.standard)
          .map((entry) => entry.path)
      )
    );
    setNames(new Map(res.data.map((entry) => [entry.path, entry.name])));
  }

  function toggle(path: string): void {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  async function importSelected(): Promise<void> {
    const chosen = (candidates() ?? []).filter((entry) => selected().has(entry.path));
    if (chosen.length === 0) return;
    setImporting(true);
    let imported = 0;
    let first: string | null = null;
    for (const entry of chosen) {
      const nickname = names().get(entry.path)?.trim();
      const res = await instancesStore.addDirectory(
        entry.path,
        nickname && nickname.length > 0 ? nickname : entry.name
      );
      if (res.ok) {
        imported += 1;
        first ??= res.data.id;
      } else {
        toast({ message: `${entry.path}: ${res.error.message}`, tone: "error" });
      }
    }
    setImporting(false);
    if (first) await instancesStore.selectDirectory(first);
    if (imported > 0) {
      toast({ message: t("instances.scan.imported", { count: imported }), tone: "success" });
      props.onClose();
    }
  }

  const importable = () =>
    (candidates() ?? []).filter((entry) => selected().has(entry.path) && !entry.registered).length;

  return (
    <RmDialog
      open={props.open}
      title={t("instances.scan.title")}
      width={700}
      onClose={props.onClose}
      footer={
        <>
          <RmButton onClick={props.onClose}>{t("common.cancel")}</RmButton>
          <RmButton
            variant="primary"
            icon="download"
            loading={importing()}
            disabled={importable() === 0}
            onClick={() => void importSelected()}
          >
            {t("instances.scan.import", { count: importable() })}
          </RmButton>
        </>
      }
    >
      <div class="dir-scan">
        <div class="dir-manage-hint">{t("instances.scan.hint")}</div>
        <Show when={loading()}>
          <div class="dir-scan-list">
            <For each={[1, 2, 3]}>{() => <div class="dir-scan-row is-skeleton" />}</For>
          </div>
        </Show>
        <Show when={!loading() && error()}>
          <RmErrorState message={error()!} onRetry={() => void scan()} />
        </Show>
        <Show when={!loading() && !error() && (candidates() ?? []).length === 0}>
          <RmEmptyState
            icon="folder"
            title={t("instances.scan.empty.title")}
            hint={t("instances.scan.empty.hint")}
          />
        </Show>
        <Show when={!loading() && !error() && (candidates() ?? []).length > 0}>
          <div class="dir-scan-list">
            <For each={candidates()!}>
              {(entry) => (
                <div
                  class={`dir-scan-row${entry.registered ? " is-registered" : ""}`}
                >
                  <RmCheckbox
                    checked={selected().has(entry.path)}
                    disabled={entry.registered}
                    onChange={() => toggle(entry.path)}
                  />
                  <div
                    class="dir-scan-main"
                    onClick={() => {
                      if (!entry.registered) toggle(entry.path);
                    }}
                  >
                    <div class="dir-scan-title">
                      <span class="dir-manage-name">{entry.name}</span>
                      <For each={entry.sources}>
                        {(source) => <RmTag>{sourceLabel(source)}</RmTag>}
                      </For>
                      <Show when={entry.preferred}>
                        <RmTag tone="accent">{t("instances.scan.preferred")}</RmTag>
                      </Show>
                      <Show when={entry.registered}>
                        <RmTag tone="success">{t("instances.scan.already")}</RmTag>
                      </Show>
                      <Show when={!entry.layout.standard && !entry.registered}>
                        <RmTag>{t("instances.dir.notStandard")}</RmTag>
                      </Show>
                    </div>
                    <div class="dir-scan-path" title={entry.path}>
                      {elidePath(entry.path, 82)}
                    </div>
                    <div class="dir-scan-meta">
                      {t("instances.manage.instanceCount", { count: entry.instance_count })}
                      <Show when={entry.layout.has_hmcl_metadata}> · HMCL</Show>
                      <Show when={entry.layout.has_pcl_metadata}> · PCL</Show>
                      <Show when={entry.layout.has_launcher_profiles}> · launcher_profiles.json</Show>
                    </div>
                    <Show when={!entry.registered}>
                      <div
                        class="dir-scan-nickname"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <input
                          class="rm-input"
                          value={names().get(entry.path) ?? entry.name}
                          placeholder={t("instances.scan.nickname")}
                          onInput={(e) =>
                            setNames((prev) => {
                              const next = new Map(prev);
                              next.set(entry.path, e.currentTarget.value);
                              return next;
                            })
                          }
                        />
                      </div>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </RmDialog>
  );
}
