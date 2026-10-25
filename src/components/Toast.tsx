/**
 * Toast layer (ui-design 10 §5): bottom-right stack, auto-dismiss,
 * optional follow-up action; `aria-live="polite"`.
 *
 * 倒计时指示为气泡底部的横向进度条（5s 从左到右填满，与自动关闭同步）；
 * 动效开关关闭时静止显示满条。曾用 SVG 包边环——WKWebView 下描边贴边
 * 被圆角/裁剪切得不完整，弃用。
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
            <div class="toast-progress" aria-hidden="true" />
          </div>
        )}
      </For>
    </div>
  );
}
