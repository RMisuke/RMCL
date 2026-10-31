/**
 * Isolated plugin UI container (Phase 4, ADR-006).
 *
 * The frame is served from `rmcccplugin://<pluginId>/...` — a different origin
 * from the shell — with a strict CSP (`connect-src 'none'`), so plugin code
 * cannot reach the host DOM, storage, or the Tauri IPC bridge. Data flows
 * through {@link PluginBridge} only.
 *
 * Failure modes are first-class and local to this card:
 *   - the document fails to load / the plugin never says `ready`  → error state
 *   - a bridge call is denied, times out or hits a protocol error → error state
 *     with the wire code, so "permission revoked" is distinguishable from a bug
 */

import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { currentThemeTokens, onThemeTokens } from "../theme";
import { t } from "../i18n";
import { PluginBridge } from "./bridge";

export interface PluginFrameProps {
  pluginId: string;
  cardId: string;
  title: string;
  /** `rmcccplugin://...` URL built by the backend (`contributions[].uiUrl`). */
  uiUrl: string;
  /** Current grid placement, exposed to the plugin in the init payload. */
  card?: { title: string; width: number; height: number };
}

/// A frame that never reports `ready` within this window is treated as broken.
const READY_TIMEOUT_MS = 8_000;

export const PluginFrame: Component<PluginFrameProps> = (props) => {
  const [status, setStatus] = createSignal<"loading" | "ready" | "error">("loading");
  const [detail, setDetail] = createSignal("");

  let frameRef: HTMLIFrameElement | undefined;
  let bridge: PluginBridge | undefined;
  let readyTimer: number | undefined;
  let unsubscribeTheme: (() => void) | undefined;

  const fail = (message: string) => {
    if (status() === "error") return;
    setStatus("error");
    setDetail(message);
    console.warn(`plugin-ui[${props.pluginId}/${props.cardId}]: ${message}`);
  };

  onMount(() => {
    const frame = frameRef;
    if (!frame) {
      fail(t("dashboard.pluginFrame.unavailable"));
      return;
    }

    bridge = new PluginBridge({
      pluginId: props.pluginId,
      cardId: props.cardId,
      card: props.card,
      frame,
      onDiagnostic: (message) => console.warn(`plugin-ui[${props.pluginId}]: ${message}`),
      onReady: () => {
        if (readyTimer !== undefined) window.clearTimeout(readyTimer);
        setStatus("ready");
      },
    });
    bridge.start();

    // A plugin that throws before registering its listener never says `ready`;
    // without this the card would show the spinner forever.
    readyTimer = window.setTimeout(() => {
      if (status() === "loading") fail(t("dashboard.pluginFrame.timeout"));
    }, READY_TIMEOUT_MS);

    unsubscribeTheme = onThemeTokens((tokens) => bridge?.sendTheme(tokens));
  });

  onCleanup(() => {
    if (readyTimer !== undefined) window.clearTimeout(readyTimer);
    unsubscribeTheme?.();
    bridge?.dispose();
  });

  const onLoad = () => {
    // A 403/404 from the asset handler loads an error page with no plugin code,
    // so `ready` never arrives; the timeout catches it. Log the hint early.
    try {
      const href = frameRef?.contentDocument?.location?.href ?? "";
      if (href.startsWith("about:")) fail(t("dashboard.pluginFrame.unavailable"));
    } catch {
      // Cross-origin access throws by design — that is the healthy case.
    }
  };

  return (
    <div class="plugin-frame">
      <Show when={status() === "loading"}>
        <div class="plugin-frame-overlay">{t("dashboard.pluginFrame.loading")}</div>
      </Show>
      <Show when={status() === "error"}>
        <div class="plugin-frame-overlay plugin-frame-overlay-error">
          <div class="plugin-frame-error-title">{t("dashboard.pluginFrame.errorTitle")}</div>
          <div class="plugin-frame-error-detail">{detail()}</div>
          <div class="plugin-frame-error-hint">{t("dashboard.pluginFrame.errorHint")}</div>
        </div>
      </Show>
      <iframe
        ref={frameRef}
        class="plugin-frame-body"
        classList={{ "is-hidden": status() === "error" }}
        title={props.title}
        src={props.uiUrl}
        // Same-origin *for its own custom scheme* (module scripts, fonts);
        // still a different origin than the host shell.
        sandbox="allow-scripts allow-same-origin"
        referrerpolicy="no-referrer"
        onLoad={onLoad}
      />
    </div>
  );
};
