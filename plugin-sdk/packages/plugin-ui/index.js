/**
 * `@rmccc/plugin-ui` — the runtime half of the isolated plugin UI bridge.
 *
 * Plain ESM with JSDoc types: usable from a bundler (import types from
 * `@rmccc/plugin-api`) **and** copyable into a no-build plugin as
 * `vendor/rmccc-plugin-ui.js`. It is served from your plugin's own
 * `rmcccplugin://` origin, so it may only use postMessage — no fetch, no host
 * APIs (the host CSP sets `connect-src 'none'`).
 *
 * Usage:
 *
 *   import { connect } from "./vendor/rmccc-plugin-ui.js";
 *
 *   const host = connect();
 *   host.onInit(({ theme, cardId }) => { applyTheme(theme); render(cardId); });
 *   const { instances } = await host.request("instances.list");
 *
 * The client sends `ready` immediately, answers `init`/`theme`/`revoke`, and
 * rejects any response whose `v` does not match the protocol version.
 */

const UI_API_VERSION = 1;

/** @typedef {"validation"|"unsupported_method"|"protocol_version"|"too_large"|"timeout"|"cancelled"|"not_installed"|"not_enabled"|"permission_denied"|"permission_pending"|"host_not_allowed"|"internal"} WireErrorCode */

/** Error thrown for host-side rejections. */
export class HostError extends Error {
  /**
   * @param {string} code
   * @param {string} message
   * @param {boolean} retryable
   */
  constructor(code, message, retryable) {
    super(message);
    this.name = "HostError";
    this.code = code;
    this.retryable = retryable;
  }
}

/**
 * Connect to the host shell.
 *
 * @param {{ requestTimeoutMs?: number }} [options]
 */
export function connect(options = {}) {
  const requestTimeoutMs = options.requestTimeoutMs ?? 10_000;
  /** @type {Map<string, {resolve: (v: unknown) => void, reject: (e: unknown) => void, timer: number}>} */
  const pending = new Map();
  /** @type {Array<(msg: any) => void>} */
  const initHandlers = [];
  /** @type {Array<(theme: any) => void>} */
  const themeHandlers = [];
  /** @type {Array<(capability: string) => void>} */
  const revokeHandlers = [];
  let nextId = 0;
  let disposed = false;
  /** @type {any} */
  let context = null;

  /** @param {unknown} message */
  function post(message) {
    window.parent.postMessage(JSON.stringify(message), "*");
  }

  /**
   * @param {string} method
   * @param {unknown} [params]
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<any>}
   */
  function request(method, params, options) {
    if (disposed) {
      return Promise.reject(new HostError("cancelled", "client disposed", false));
    }
    const signal = options?.signal;
    if (signal?.aborted) {
      return Promise.reject(new HostError("cancelled", "request aborted before it started", false));
    }

    const id = "r" + ++nextId;
    return new Promise((resolve, reject) => {
      /** Cancel locally and tell the host to drop the pending entry. */
      const cancel = (reason) => {
        const entry = pending.get(id);
        if (!entry) return;
        pending.delete(id);
        window.clearTimeout(entry.timer);
        signal?.removeEventListener("abort", onAbort);
        // Fire-and-forget: the host discards any late result for this id.
        post({ v: UI_API_VERSION, type: "cancel", id });
        reject(new HostError("cancelled", reason, false));
      };
      const onAbort = () => cancel("request cancelled by the plugin");

      const timer = window.setTimeout(() => {
        pending.delete(id);
        signal?.removeEventListener("abort", onAbort);
        reject(new HostError("timeout", method + " timed out", true));
      }, requestTimeoutMs);
      pending.set(id, { resolve, reject, timer });
      signal?.addEventListener("abort", onAbort, { once: true });
      post({ v: UI_API_VERSION, id, method, params: params ?? {} });
    });
  }

  function onMessage(/** @type {MessageEvent} */ event) {
    if (typeof event.data !== "string") return;
    /** @type {any} */
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (!message || message.v !== UI_API_VERSION) return;

    if (message.type === "init") {
      context = message;
      for (const handler of initHandlers) safe(handler, message);
      return;
    }
    if (message.type === "theme") {
      for (const handler of themeHandlers) safe(handler, message.theme);
      return;
    }
    if (message.type === "revoke") {
      for (const handler of revokeHandlers) safe(handler, message.capability);
      return;
    }

    const entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    window.clearTimeout(entry.timer);
    if (message.ok) {
      entry.resolve(message.result);
    } else {
      const error = message.error ?? {};
      entry.reject(new HostError(error.code ?? "internal", error.message ?? "unknown error", !!error.retryable));
    }
  }

  function safe(/** @type {Function} */ handler, /** @type {unknown} */ value) {
    try {
      handler(value);
    } catch (e) {
      console.error("rmccc plugin handler failed", e);
    }
  }

  window.addEventListener("message", onMessage);

  return {
    /** Protocol version in use. */
    apiVersion: UI_API_VERSION,
    /** The `init` payload (null until the host answers). */
    get context() {
      return context;
    },
    /** @param {(msg: any) => void} handler */
    onInit(handler) {
      if (context) safe(handler, context);
      else initHandlers.push(handler);
      return () => {
        const i = initHandlers.indexOf(handler);
        if (i >= 0) initHandlers.splice(i, 1);
      };
    },
    /** @param {(theme: any) => void} handler */
    onTheme(handler) {
      themeHandlers.push(handler);
      return () => {
        const i = themeHandlers.indexOf(handler);
        if (i >= 0) themeHandlers.splice(i, 1);
      };
    },
    /** @param {(capability: string) => void} handler */
    onRevoke(handler) {
      revokeHandlers.push(handler);
      return () => {
        const i = revokeHandlers.indexOf(handler);
        if (i >= 0) revokeHandlers.splice(i, 1);
      };
    },
    request,
    /**
     * Cancel one in-flight request by id. Prefer `request(method, params,
     * { signal })`: the wrapper cancels for you on `AbortSignal.abort`.
     */
    cancel(id) {
      const entry = pending.get(id);
      if (!entry) return false;
      pending.delete(id);
      window.clearTimeout(entry.timer);
      post({ v: UI_API_VERSION, type: "cancel", id });
      entry.reject(new HostError("cancelled", "request cancelled", false));
      return true;
    },
    /** Convenience wrappers for the v1 method table. */
    info: () => request("host.info"),
    theme: () => request("theme.get"),
    settings: () => request("settings.get"),
    instances: () => request("instances.list"),
    tasks: () => request("tasks.list"),
    launch: () => request("launch.status"),
    /** Query a declared Minecraft server through the host's SLP bridge. */
    server: (params) => request("server.status", params),
    /**
     * @param {{url: string, method?: "GET"|"POST", body?: string}} params
     */
    network: (params) => request("network.request", params),
    /** Stop listening and reject outstanding requests. */
    dispose() {
      disposed = true;
      window.removeEventListener("message", onMessage);
      for (const [, entry] of pending) {
        window.clearTimeout(entry.timer);
        entry.reject(new HostError("cancelled", "client disposed", false));
      }
      pending.clear();
    },
  };
}

/**
 * Apply host theme tokens as CSS variables.
 *
 * @param {any} theme
 * @param {HTMLElement} [target]
 */
export function applyTheme(theme, target = document.documentElement) {
  if (!theme) return;
  const style = target.style;
  style.setProperty("--bg", theme.background);
  style.setProperty("--surface", theme.surface);
  style.setProperty("--surface-elevated", theme.surfaceElevated);
  style.setProperty("--text", theme.text);
  style.setProperty("--muted", theme.textMuted);
  style.setProperty("--accent", theme.accent);
  style.setProperty("--accent-contrast", theme.accentContrast);
  style.setProperty("--border", theme.border);
}

/** Announce the frame to the host. Call once, after registering handlers. */
export function announce() {
  window.parent.postMessage(JSON.stringify({ v: UI_API_VERSION, type: "ready" }), "*");
}
