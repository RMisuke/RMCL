/**
 * Isolated plugin UI container — minimal skeleton.
 *
 * The previous host bridge (postMessage protocol, theme push, quota control)
 * was retired for a redesign. This component only mounts the sandboxed iframe
 * and resolves the platform-correct custom-scheme URL; the backend command and
 * `rmcccplugin://` scheme handler stay in place as the framework for the next
 * architecture iteration.
 */
import type { Component, JSX } from "solid-js";
import { Show, createSignal } from "solid-js";
import { t } from "../i18n";

export interface PluginFrameProps {
  pluginId: string;
  title: string;
  /** Path inside the plugin's `ui/` directory, e.g. `index.html?page=settings`. */
  entry: string;
  class?: string;
}

/**
 * WebView2 does not resolve arbitrary schemes; Tauri serves registered custom
 * protocols as `http://<scheme>.localhost` on Windows.
 */
export function pluginUiUrl(pluginId: string, entry: string): string {
  const path = entry.replace(/^\/+/, "");
  return /Windows/i.test(navigator.userAgent)
    ? `http://rmcccplugin.localhost/${pluginId}/${path}`
    : `rmcccplugin://${pluginId}/${path}`;
}

export const PluginFrame: Component<PluginFrameProps> = (props): JSX.Element => {
  const [failed, setFailed] = createSignal(false);

  return (
    <div class={`plugin-frame${props.class ? ` ${props.class}` : ""}`}>
      <Show when={failed()}>
        <div class="plugin-frame-overlay plugin-frame-overlay-error">
          <div class="plugin-frame-error-title">{t("dashboard.pluginFrame.errorTitle")}</div>
          <div class="plugin-frame-error-hint">{t("dashboard.pluginFrame.errorHint")}</div>
        </div>
      </Show>
      <iframe
        class="plugin-frame-body"
        classList={{ "is-hidden": failed() }}
        title={props.title}
        src={pluginUiUrl(props.pluginId, props.entry)}
        sandbox="allow-scripts allow-same-origin"
        referrerpolicy="no-referrer"
        onError={() => setFailed(true)}
      />
    </div>
  );
};
