/**
 * Home server-info card backed by the real Rust SLP query (`server_status`).
 *
 * No mock data, no synthetic "online": an empty address stays idle, a failed
 * query shows the backend error, and a successful query shows MOTD, players,
 * latency, version and the server favicon.
 */
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import type { JSX } from "solid-js";
import { Icon } from "../icons";
import { RmButton, RmIconButton, RmInput } from "../rm/controls";
import { servers } from "../../ipc/api";
import type { ServerStatus } from "../../ipc/types";
import { t } from "../../i18n";

const ADDRESS_KEY = "rmccc.serverQuery.v1";
const REFRESH_MS = 90_000;

function readAddress(): string {
  try {
    return localStorage.getItem(ADDRESS_KEY) ?? "";
  } catch {
    return "";
  }
}

function saveAddress(address: string): void {
  try {
    localStorage.setItem(ADDRESS_KEY, address);
  } catch {
    // Keep the query usable without persistence.
  }
}

export function ServerCard(): JSX.Element {
  const [address, setAddress] = createSignal(readAddress());
  const [status, setStatus] = createSignal<ServerStatus | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let requestId = 0;
  let timer: ReturnType<typeof setInterval> | undefined;

  async function query(): Promise<void> {
    const value = address().trim();
    if (!value || loading()) return;
    const id = ++requestId;
    setLoading(true);
    setError(null);
    saveAddress(value);
    const res = await servers.status(value);
    if (id !== requestId) return;
    setLoading(false);
    if (res.ok) {
      setStatus(res.data);
      setAddress(value);
    } else {
      setStatus(null);
      setError(res.error.message);
    }
  }

  onMount(() => {
    if (address().trim()) void query();
    timer = setInterval(() => {
      if (document.visibilityState === "visible" && status() && !loading()) void query();
    }, REFRESH_MS);
  });

  onCleanup(() => {
    requestId += 1;
    if (timer) clearInterval(timer);
  });

  const updatedAt = () => {
    const value = status()?.queriedAt;
    if (!value) return "";
    return new Date(value).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  return (
    <div class="server-card">
      <div class="server-query">
        <RmInput
          value={address()}
          onInput={setAddress}
          placeholder={t("hero.server.address")}
          onEnter={() => void query()}
          aria-label={t("hero.server.address")}
        />
        <RmButton
          size="sm"
          variant="primary"
          icon="search"
          loading={loading()}
          onClick={() => void query()}
        >
          {t("hero.server.connect")}
        </RmButton>
        <RmIconButton
          icon="refresh"
          label={t("hero.server.refresh")}
          disabled={loading() || !address().trim()}
          onClick={() => void query()}
        />
      </div>

      <Show when={error()}>
        <div class="server-error">
          <Icon name="alert" size={15} />
          <span>{error()}</span>
          <button type="button" onClick={() => void query()}>{t("hero.server.retry")}</button>
        </div>
      </Show>

      <Show when={status()}>
        {(data) => (
          <div class="server-summary">
            <Show
              when={data().favicon}
              fallback={
                <span class="server-favicon is-fallback" aria-hidden="true">
                  <Icon name="activity" size={20} />
                </span>
              }
            >
              {(icon) => (
                <img
                  class="server-favicon"
                  src={icon()}
                  alt={t("hero.server.favicon")}
                  width="34"
                  height="34"
                />
              )}
            </Show>
            <div class="server-copy">
              <strong class="server-motd">{data().motd}</strong>
              <div class="server-meta">
                <span><Icon name="person" size={12} />{t("hero.server.players", { online: data().online, max: data().max })}</span>
                <span><Icon name="activity" size={12} />{t("hero.server.latency", { ms: data().latencyMs })}</span>
                <span class="server-version" title={`${data().host}:${data().port}`}>{data().version}</span>
              </div>
            </div>
            <span class="server-checked">{updatedAt()}</span>
          </div>
        )}
      </Show>

      <Show when={(status()?.sample?.length ?? 0) > 0}>
        <div class="server-sample">
          <span class="server-sample-label">{t("hero.server.sample")}</span>
          <For each={status()!.sample}>
            {(name) => <span class="server-player">{name}</span>}
          </For>
        </div>
      </Show>

      <Show when={status()}>
        {(data) => (
          <div class="server-footer">
            <span class="server-endpoint" title={`${data().host}:${data().port}`}>{data().host}:{data().port}</span>
            <span>{t("hero.server.protocol", { protocol: data().protocol })}</span>
            <span>{t("hero.server.autoRefresh")}</span>
          </div>
        )}
      </Show>

      <Show when={!status() && !error() && !loading()}>
        <div class="soft-empty server-idle">
          <Icon name="info" size={16} />
          <span>{t("hero.server.idle")}</span>
          <small>{t("hero.server.srvHint")}</small>
        </div>
      </Show>
      <Show when={loading() && !status()}>
        <div class="soft-empty server-idle">
          <span class="server-spinner" aria-hidden="true" />
          {t("hero.server.probing")}
        </div>
      </Show>
    </div>
  );
}
