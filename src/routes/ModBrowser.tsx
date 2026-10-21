/**
 * Mod Browser (Phase 0.5.2) — search Modrinth, pick a version, install into an
 * instance. Reuses the existing IPC surface (`mod_search` / `mod_get_versions` /
 * `mod_install`) through the typed `api.ts` wrappers; no new backend commands.
 */
import { Component, For, Show, createSignal, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { mods, profiles } from "../ipc/api";
import type { ModInfo, ModVersion, Profile } from "../ipc/types";
import { showToast } from "../components/Toast";
import { t } from "../i18n";

const PAGE_SIZE = 20;

/** 1234567 → "1.2M"; keeps long download counts readable in the list. */
function formatCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

const ModBrowser: Component = () => {
  const navigate = useNavigate();

  const [profileList, setProfileList] = createSignal<Profile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = createSignal<string | null>(null);

  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<ModInfo[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [loading, setLoading] = createSignal(false);
  const [searched, setSearched] = createSignal(false);

  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [versions, setVersions] = createSignal<ModVersion[]>([]);
  const [versionsLoading, setVersionsLoading] = createSignal(false);
  const [installing, setInstalling] = createSignal<string | null>(null);

  onMount(async () => {
    const r = await profiles.list();
    if (r.ok) {
      setProfileList(r.data);
      if (r.data.length > 0) setSelectedProfileId(r.data[0].id);
    } else {
      showToast(r.error);
    }
  });

  const totalPages = () => Math.max(1, Math.ceil(total() / PAGE_SIZE));

  const runSearch = async (nextPage: number) => {
    const q = query().trim();
    if (!q) return;
    setLoading(true);
    const r = await mods.search(q, undefined, undefined, nextPage, PAGE_SIZE);
    setLoading(false);
    setSearched(true);
    if (r.ok) {
      setHits(r.data.hits);
      setTotal(r.data.total);
      setPage(r.data.page);
      setExpandedId(null);
    } else {
      showToast(r.error);
    }
  };

  const toggleExpand = async (mod: ModInfo) => {
    if (expandedId() === mod.id) {
      setExpandedId(null);
      return;
    }
    setExpandedId(mod.id);
    setVersionsLoading(true);
    const r = await mods.getVersions(mod.id);
    setVersionsLoading(false);
    if (r.ok) {
      setVersions(r.data);
    } else {
      setVersions([]);
      showToast(r.error);
    }
  };

  const install = async (version: ModVersion) => {
    const pid = selectedProfileId();
    if (!pid) {
      showToast({ code: "validation_error", message: t("mods.selectInstanceFirst"), retryable: false });
      return;
    }
    setInstalling(version.id);
    const r = await mods.install(pid, version);
    setInstalling(null);
    if (r.ok) {
      showToast({ code: "success", message: t("mods.installed"), retryable: false });
    } else {
      showToast(r.error);
    }
  };

  return (
    <div class="mod-browser">
      <div class="mod-browser-header">
        <button class="modal-btn-cancel" onClick={() => navigate("/")}>
          {t("common.back")}
        </button>
        <h1 class="mod-browser-title">{t("mods.title")}</h1>
        <label class="mod-instance-picker">
          <span>{t("mods.instance")}</span>
          <select
            class="modal-input"
            value={selectedProfileId() ?? ""}
            onChange={(e) => setSelectedProfileId(e.currentTarget.value || null)}
          >
            <For each={profileList()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
          </select>
        </label>
      </div>

      <form
        class="mod-search-row"
        onSubmit={(e) => {
          e.preventDefault();
          runSearch(1);
        }}
      >
        <input
          class="modal-input"
          placeholder={t("mods.searchPlaceholder")}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
        />
        <button class="modal-btn-primary" type="submit" disabled={loading()}>
          {loading() ? t("common.loading") : t("mods.search")}
        </button>
      </form>

      <Show when={searched() && !loading() && hits().length === 0}>
        <div class="empty-state">
          <div class="empty-message">{t("mods.noResults")}</div>
        </div>
      </Show>

      <div class="mod-results">
        <For each={hits()}>
          {(mod) => (
            <div class="mod-item">
              <button class="mod-item-head" onClick={() => toggleExpand(mod)}>
                <Show when={mod.icon_url}>
                  <img class="mod-icon" src={mod.icon_url!} alt="" />
                </Show>
                <div class="mod-meta">
                  <div class="mod-name">{mod.name}</div>
                  <div class="mod-author">{mod.authors.join(", ")}</div>
                  <div class="mod-desc">{mod.description}</div>
                </div>
                <div class="mod-downloads">{formatCount(mod.download_count)}</div>
              </button>

              <Show when={expandedId() === mod.id}>
                <div class="mod-versions">
                  <Show
                    when={!versionsLoading()}
                    fallback={<div class="loading-inline">{t("common.loading")}</div>}
                  >
                    <For each={versions()}>
                      {(v) => (
                        <div class="mod-version-row">
                          <span class="mod-version-name">{v.version_number}</span>
                          <span class="mod-version-games">
                            {v.game_versions.slice(0, 4).join(", ")}
                          </span>
                          <button
                            class="modal-btn-primary"
                            disabled={installing() === v.id || !selectedProfileId()}
                            onClick={() => install(v)}
                          >
                            {installing() === v.id ? t("mods.installing") : t("mods.install")}
                          </button>
                        </div>
                      )}
                    </For>
                  </Show>
                </div>
              </Show>
            </div>
          )}
        </For>
      </div>

      <Show when={total() > PAGE_SIZE}>
        <div class="mod-pager">
          <button
            class="modal-btn-cancel"
            disabled={page() <= 1}
            onClick={() => runSearch(page() - 1)}
          >
            ‹
          </button>
          <span>
            {page()} / {totalPages()}
          </span>
          <button
            class="modal-btn-cancel"
            disabled={page() >= totalPages()}
            onClick={() => runSearch(page() + 1)}
          >
            ›
          </button>
        </div>
      </Show>
    </div>
  );
};

export default ModBrowser;
