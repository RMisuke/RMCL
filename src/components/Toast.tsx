/**
 * Toast layer (ui-design 10 \u00a75): bottom-right stack, 4s auto-dismiss,
 * optional follow-up action; `aria-live="polite"`.
 */
import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { Icon, type IconName } from "./icons";
import { uiStore } from "../stores/ui";
import { t } from "../i18n";

const TONE_ICON: Record<string, IconName> = {
  success: "check",
  error: "alert",
  info: "info",
};

export function ToastContainer(): JSX.Element {
  return (
    <div class="toast-container" aria-live="polite">
      <For each={uiStore.toasts()}>
        {(item) => (
          <div class={`toast toast-${item.tone}`} role="status">
            <Icon name={TONE_ICON[item.tone] ?? "info"} size={16} />
            <span class="toast-message">{item.message}</span>
            <Show when={item.actionLabel}>
              <button
                class="toast-action"
                onClick={() => {
                  item.onAction?.();
                  uiStore.dismissToast(item.id);
                }}
              >
                {item.actionLabel}
              </button>
            </Show>
            <button
              class="toast-close"
              aria-label={t("common.close")}
              onClick={() => uiStore.dismissToast(item.id)}
            >
              <Icon name="x" size={13} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
