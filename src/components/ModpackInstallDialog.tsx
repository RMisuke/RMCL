/**
 * Modpack install wizard (HMCL `LocalModpackPage` / `ModpackPage`): shows the
 * parsed manifest, lets the user name the instance and pick a game directory,
 * then hands installation to the backend.
 *
 * Opened by the global drop handler and the Download center's modpack tab.
 */
import type { JSX } from "solid-js";
import { For, Show, createEffect, createMemo, createSignal, onMount } from "solid-js";
import { RmButton, RmInput, RmSelect, RmTag } from "./rm/controls";
import { RmDialog } from "./rm/overlays";
import { Icon } from "./icons";
import { t } from "../i18n";
import { instancesStore } from "../stores/instances";
import { modpacksStore } from "../stores/modpacks";
import { uiStore } from "../stores/ui";

export function ModpackInstallDialog(): JSX.Element {
  const [name, setName] = createSignal("");
  const [directoryId, setDirectoryId] = createSignal("");

  const inspection = () => modpacksStore.inspection();

  onMount(() => {
    // The dialog is reachable from any route; the directory list may not have
    // been loaded yet when the user goes straight to Download → Modpacks.
    if (!instancesStore.directories()) void instancesStore.refreshDirectories(true);
  });
  const directories = createMemo(() => instancesStore.directories() ?? []);
  const directoryOptions = createMemo(() =>
    directories().map((entry) => ({
      value: entry.directory.id,
      label: entry.directory.name,
    }))
  );

  // A new archive resets the form to the manifest's suggested id.
  createEffect(() => {
    const current = inspection();
    if (!current) return;
    setName(current.suggested_id);
    const selected = instancesStore.selectedDirectory();
    setDirectoryId(selected?.directory.id ?? directoryOptions()[0]?.value ?? "");
  });

  // Directories may arrive after the dialog opens (store bootstrap); fill the
  // first option once instead of leaving the install button disabled.
  createEffect(() => {
    if (!inspection()) return;
    if (directoryId()) return;
    const first = directoryOptions()[0]?.value;
    if (first) setDirectoryId(first);
  });

  const nameError = createMemo(() => {
    const value = name().trim();
    if (!value) return t("modpack.name.required");
    if (/[\\/:*?"<>|]/.test(value)) return t("modpack.name.invalid");
    return null;
  });

  const canInstall = createMemo(() => {
    const current = inspection();
    if (!current) return false;
    if (nameError()) return false;
    if (!current.is_manual && !directoryId()) return false;
    return !modpacksStore.starting();
  });

  async function submit(): Promise<void> {
    const current = inspection();
    if (!current || !canInstall()) return;
    if (await modpacksStore.startInstall(directoryId(), name().trim())) {
      uiStore.setTaskCenterOpen(true);
    }
  }

  return (
    <RmDialog
      open={!!inspection()}
      title={t("modpack.installTitle")}
      width={560}
      onClose={() => modpacksStore.dismissInspection()}
      footer={
        <>
          <RmButton variant="ghost" onClick={() => modpacksStore.dismissInspection()}>
            {t("common.cancel")}
          </RmButton>
          <RmButton
            variant="primary"
            icon="download"
            loading={modpacksStore.starting()}
            disabled={!canInstall()}
            onClick={() => void submit()}
          >
            {t("modpack.installNow")}
          </RmButton>
        </>
      }
    >
      <Show when={inspection()} keyed>
        {(pack) => (
          <div class="modpack-inspect">
            <div class="modpack-inspect-head">
              <div class="modpack-cover" aria-hidden="true">
                <Show
                  when={pack.icon_data_url}
                  fallback={<Icon name="layers" size={30} />}
                >
                  <img src={pack.icon_data_url!} alt="" />
                </Show>
              </div>
              <div class="modpack-inspect-copy">
                <div class="modpack-inspect-title">
                  <h4>{pack.name}</h4>
                  <RmTag tone="accent">{pack.kind_name}</RmTag>
                </div>
                <div class="modpack-inspect-meta">
                  <Show when={pack.game_version}>
                    <span>Minecraft {pack.game_version}</span>
                  </Show>
                  <Show when={pack.version}>
                    <span>v{pack.version}</span>
                  </Show>
                  <Show when={pack.author}>
                    <span>{t("modpack.author")}: {pack.author}</span>
                  </Show>
                </div>
                <div class="modpack-inspect-loaders">
                  <For each={pack.loaders}>
                    {(loader) => (
                      <RmTag tone="neutral">
                        {loader.name} {loader.version}
                      </RmTag>
                    )}
                  </For>
                </div>
              </div>
            </div>

            <Show when={pack.description}>
              <div class="modpack-description">{pack.description}</div>
            </Show>

            <Show when={pack.warnings.length > 0}>
              <div class="modpack-warnings">
                <For each={pack.warnings}>
                  {(warning) => (
                    <div class="modpack-warning">
                      <Icon name="alert" size={14} />
                      <span>{warning}</span>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <Show when={pack.is_manual}>
              <div class="modpack-manual-note">
                <Icon name="info" size={15} />
                <span>{t("modpack.manualHint")}</span>
              </div>
            </Show>

            <div class="rm-field">
              <div class="rm-field-label">{t("modpack.nameLabel")}</div>
              <RmInput
                value={name()}
                onInput={setName}
                placeholder={pack.suggested_id}
                error={nameError() ?? undefined}
                onEnter={() => void submit()}
              >
                {null}
              </RmInput>
              <div class="rm-field-hint">{t("modpack.nameHint")}</div>
            </div>

            <Show when={!pack.is_manual}>
              <div class="rm-field">
                <div class="rm-field-label">{t("modpack.directoryLabel")}</div>
                <RmSelect
                  value={directoryId()}
                  onChange={setDirectoryId}
                  options={directoryOptions()}
                  disabled={directoryOptions().length === 0}
                />
                <Show when={directoryOptions().length === 0}>
                  <div class="rm-field-error">{t("modpack.directoryEmpty")}</div>
                </Show>
              </div>
            </Show>

            <div class="modpack-facts">
              <Show when={pack.file_count > 0}>
                <span>
                  <Icon name="package" size={14} />
                  {t("modpack.fileCount", { n: String(pack.file_count) })}
                </span>
              </Show>
              <span class="modpack-path" title={pack.path}>
                {pack.path}
              </span>
            </div>
          </div>
        )}
      </Show>
    </RmDialog>
  );
}
