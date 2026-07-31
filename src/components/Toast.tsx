import { Component, createSignal, createRoot, Show, For } from "solid-js";
import type { IpcError } from "../ipc/api";

// ── Toast store (global, wrapped in createRoot for clean HMR) ──

interface Toast {
  id: number;
  error: IpcError;
  timestamp: number;
}

const { toasts, showToast, dismissToast } = createRoot(() => {
  const [toasts, setToasts] = createSignal<Toast[]>([]);
  let nextId = 0;

  function showToast(error: IpcError) {
    const id = nextId++;
    setToasts((prev) => [...prev, { id, error, timestamp: Date.now() }]);

    // Auto-dismiss after 5 seconds (unless retryable)
    if (!error.retryable) {
      setTimeout(() => dismissToast(id), 5000);
    }
  }

  function dismissToast(id: number) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  return { toasts, showToast, dismissToast };
});

export { showToast, dismissToast };

// ── Toast component ──

export const ToastContainer: Component = () => {
  return (
    <div class="toast-container">
      <For each={toasts()}>
        {(toast) => <ToastItem toast={toast} />}
      </For>
    </div>
  );
};

const ToastItem: Component<{ toast: Toast }> = (props) => {
  const error = () => props.toast.error;

  const getIcon = () => {
    switch (error().code) {
      case "java_not_found":
        return "⚠️";
      case "auth_failed":
      case "token_expired":
        return "🔑";
      case "download_failed":
      case "network_error":
        return "📡";
      case "insufficient_disk_space":
        return "💾";
      default:
        return "❌";
    }
  };

  const handleRetry = () => {
    // TODO: Implement retry logic (re-invoke the failed command)
    dismissToast(props.toast.id);
  };

  return (
    <div class="toast-item">
      <div class="toast-icon">{getIcon()}</div>
      <div class="toast-content">
        <div class="toast-message">{error().message}</div>
        <Show when={error().retryable}>
          <button class="toast-retry" onClick={handleRetry}>
            Retry
          </button>
        </Show>
      </div>
      <button
        class="toast-close"
        onClick={() => dismissToast(props.toast.id)}
      >
        ×
      </button>
    </div>
  );
};
