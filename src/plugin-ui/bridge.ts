/**
 * Host side of the isolated plugin UI bridge (Phase 4, ADR-006).
 *
 * The frame is served from `rmcccplugin://<pluginId>/`, a different origin than
 * the shell (`tauri://localhost`), so it cannot touch host DOM or the injected
 * Tauri API. Everything it needs comes through this class:
 *
 *   frame --ready--> host --init--> frame
 *   frame --request--> host --plugin_ui_invoke--> Rust --reply--> frame
 *
 * What the host does **not** do: authorize. Every forwarded call is re-checked in
 * `plugin_ui_invoke` against the live registry/permission state; this class only
 * enforces transport-level rules (source, size, in-flight quota, timeouts).
 */

import { pluginUi } from "../ipc/api";
import { currentThemeTokens } from "../theme";
import type { BridgeApiError, ThemeTokens } from "../ipc/types";

/// Must match `UI_API_VERSION` in `crates/rmccc-plugin/src/ui.rs`.
export const UI_API_VERSION = 1;

/// Must match `MAX_REQUEST_BYTES` / `MAX_IN_FLIGHT` in the same module.
const MAX_REQUEST_BYTES = 64 * 1024;
const MAX_IN_FLIGHT = 8;

/// Timeouts mirror `BRIDGE_METHODS` in `crates/rmccc-plugin/src/ui.rs`.
/// They are a UI guard only: the host command applies its own limits.
const METHOD_TIMEOUTS: Record<string, number> = {
  "host.info": 5_000,
  "theme.get": 5_000,
  "settings.get": 10_000,
  "instances.list": 10_000,
  "tasks.list": 10_000,
  "launch.status": 5_000,
  "server.status": 30_000,
  "network.request": 30_000,
  // ADR-013 S6: streaming replies arrive as `llm.delta/done/error` pushes;
  // this timeout only bounds the non-streaming completion wait.
  "llm.chat": 30_000,
};

/// Methods answered by the shell itself (no IPC round-trip). The shell applies
/// the tokens, so it is the only place that knows the effective palette.
const LOCAL_METHODS = new Set(["theme.get"]);

/**
 * Live bridges by plugin id, for host→plugin push relay (ADR-013 S6:
 * `plugin://push` events carrying `{v, type: "llm.delta"|"llm.done"|"llm.error"}`).
 * A plugin may own several frames (card + settings page); the push goes to all.
 */
const liveBridges = new Map<string, Set<PluginBridge>>();

/** Relay a host→plugin push to every live frame of `pluginId`. */
export function dispatchPluginPush(pluginId: string, message: unknown): void {
  for (const bridge of liveBridges.get(pluginId) ?? []) {
    bridge.sendPush(message);
  }
}

export interface BridgeCardContext {
  title: string;
  width: number;
  height: number;
}

export interface BridgeOptions {
  pluginId: string;
  cardId: string;
  card?: BridgeCardContext;
  frame: HTMLIFrameElement;
  /** Diagnostics sink (toast/log); never throws into the message handler. */
  onDiagnostic?: (message: string) => void;
  /** Called once the plugin announced itself and the init payload was sent. */
  onReady?: () => void;
}

interface WireMessage {
  v?: number;
  type?: string;
  id?: string;
  method?: string;
  params?: unknown;
}

export class PluginBridge {
  private readonly pluginId: string;
  private readonly cardId: string;
  private readonly card?: BridgeCardContext;
  private readonly frame: HTMLIFrameElement;
  private readonly onDiagnostic?: (message: string) => void;
  private readonly onReady?: () => void;
  private readonly pending = new Set<string>();
  private readonly timers = new Map<string, number>();
  /**
   * Concrete origin learned from incoming messages. WebKit reports opaque
   * ("null") origins for these sandboxed custom-scheme frames — those cannot
   * be named in `targetOrigin`, so replies fall back to "*" while the
   * navigation seal below is what protects them. On engines that do expose a
   * real origin we pin it and cross-origin delivery becomes impossible.
   */
  private pinnedOrigin: string | null = null;
  /**
   * P1-1 (plugin security audit): a sandboxed frame may navigate itself
   * anywhere. The FIRST `load` is the plugin's own package document; any
   * later `load` means the window now hosts a different document, and
   * in-flight replies must never be delivered to it. We drop all pending
   * requests and seal the bridge (post() becomes a no-op until remount).
   */
  private initialLoadDone = false;
  private sealed = false;
  private readonly onFrameLoad = () => {
    if (!this.initialLoadDone) {
      this.initialLoadDone = true;
      return;
    }
    this.sealed = true;
    for (const timer of this.timers.values()) window.clearTimeout(timer);
    this.timers.clear();
    this.pending.clear();
    this.diagnostic("frame navigated away from its package document; bridge sealed");
  };
  private started = false;
  private disposed = false;
  private readonly listener = (event: MessageEvent) => this.onMessage(event);

  constructor(options: BridgeOptions) {
    this.pluginId = options.pluginId;
    this.cardId = options.cardId;
    this.card = options.card;
    this.frame = options.frame;
    this.onDiagnostic = options.onDiagnostic;
    this.onReady = options.onReady;
  }

  start(): void {
    if (this.started) return;
    this.started = true;
    window.addEventListener("message", this.listener);
    this.frame.addEventListener("load", this.onFrameLoad);
    let set = liveBridges.get(this.pluginId);
    if (!set) {
      set = new Set();
      liveBridges.set(this.pluginId, set);
    }
    set.add(this);
  }

  dispose(): void {
    this.disposed = true;
    if (this.started) {
      window.removeEventListener("message", this.listener);
      this.frame.removeEventListener("load", this.onFrameLoad);
      const set = liveBridges.get(this.pluginId);
      set?.delete(this);
      if (set && set.size === 0) liveBridges.delete(this.pluginId);
    }
    for (const timer of this.timers.values()) window.clearTimeout(timer);
    this.timers.clear();
    this.pending.clear();
  }

  /** Push a theme change to a live frame (no-op until it said `ready`). */
  sendTheme(tokens: ThemeTokens): void {
    this.post({ v: UI_API_VERSION, type: "theme", theme: tokens });
  }

  /** Host→plugin push (llm.chat streaming etc.); no-op when disposed. */
  sendPush(message: unknown): void {
    if (!this.disposed) this.post(message);
  }

  private onMessage(event: MessageEvent): void {
    if (this.disposed || this.sealed) return;
    // The load-bearing check: only the frame window we created may talk to us.
    if (event.source !== this.frame.contentWindow) return;
    // `sandbox`ed custom-scheme frames report an opaque origin ("null") in
    // WebKit. When the engine exposes a concrete origin it must be **this
    // plugin's own** origin — accepting any `rmcccplugin://` origin would let a
    // frame that navigated to another plugin's document talk to this bridge
    // under this plugin's capabilities (plugin security audit, 2026-10-27).
    const ownOrigin = `rmcccplugin://${this.pluginId}`;
    if (event.origin !== "null" && event.origin !== ownOrigin) {
      this.diagnostic(`dropped message from unexpected origin ${event.origin}`);
      return;
    }
    // Learn the concrete origin when the engine exposes one (see pinnedOrigin).
    if (event.origin === ownOrigin) this.pinnedOrigin = event.origin;

    const raw = event.data;
    if (typeof raw !== "string") {
      this.diagnostic("dropped non-string message (plugins must post JSON strings)");
      return;
    }
    if (raw.length > MAX_REQUEST_BYTES) {
      this.diagnostic(`dropped oversized message (${raw.length} bytes)`);
      return;
    }

    let msg: WireMessage;
    try {
      msg = JSON.parse(raw) as WireMessage;
    } catch {
      this.diagnostic("dropped malformed message (invalid JSON)");
      return;
    }

    if (msg.type === "ready") {
      this.sendInit();
      this.onReady?.();
      return;
    }
    // Plugin-initiated cancellation: drop the pending entry so a late reply is
    // discarded (no response is sent for the cancelled id).
    if (msg.type === "cancel") {
      if (typeof msg.id !== "string") {
        this.diagnostic("dropped cancel message without id");
        return;
      }
      if (this.pending.delete(msg.id)) {
        this.clearTimer(msg.id);
        this.diagnostic(`request ${msg.id} cancelled by the plugin`);
      }
      return;
    }

    if (typeof msg.id !== "string" || typeof msg.method !== "string") {
      this.diagnostic("dropped message without id/method");
      return;
    }
    void this.handleRequest(msg as Required<Pick<WireMessage, "id" | "method">> & WireMessage, raw);
  }

  private sendInit(): void {
    this.post({
      v: UI_API_VERSION,
      type: "init",
      pluginId: this.pluginId,
      cardId: this.cardId,
      ...(this.card ? { card: this.card } : {}),
      theme: currentThemeTokens() ?? {},
      limits: {
        maxRequestBytes: MAX_REQUEST_BYTES,
        maxResponseBytes: 512 * 1024,
        timeoutMs: 10_000,
      },
    });
  }

  private async handleRequest(msg: WireMessage, raw: string): Promise<void> {
    const id = msg.id as string;
    const method = msg.method as string;

    if (msg.v !== UI_API_VERSION) {
      return this.respondError(id, {
        code: "protocol_version",
        message: `unsupported UI API version ${String(msg.v)}`,
        retryable: false,
      });
    }
    if (!(method in METHOD_TIMEOUTS)) {
      return this.respondError(id, {
        code: "unsupported_method",
        message: `unknown method ${method}`,
        retryable: false,
      });
    }
    if (this.pending.size >= MAX_IN_FLIGHT) {
      return this.respondError(id, {
        code: "validation",
        message: "too many in-flight requests",
        retryable: true,
      });
    }

    if (LOCAL_METHODS.has(method)) {
      return this.respondLocal(id, method);
    }

    const timer = window.setTimeout(() => {
      this.pending.delete(id);
      this.timers.delete(id);
      this.respondError(id, {
        code: "timeout",
        message: `no reply within ${METHOD_TIMEOUTS[method]} ms`,
        retryable: true,
      });
    }, METHOD_TIMEOUTS[method]);
    this.pending.add(id);
    this.timers.set(id, timer);

    try {
      const res = await pluginUi.invoke(this.pluginId, raw);
      const envelope = res.ok ? res.data : null;
      if (this.pending.delete(id)) {
        this.clearTimer(id);
        if (envelope?.ok) this.respondOk(id, this.decorate(method, envelope.result));
        else this.respondError(id, envelope?.error ?? null);
      }
    } catch (e) {
      if (this.pending.delete(id)) {
        this.clearTimer(id);
        this.respondError(id, {
          code: "internal",
          message: `host call failed: ${String(e)}`,
          retryable: true,
        });
      }
    }
  }

  private respondLocal(id: string, method: string): void {
    if (method !== "theme.get") {
      return this.respondError(id, {
        code: "unsupported_method",
        message: `method ${method} is not handled locally`,
        retryable: false,
      });
    }
    const tokens = currentThemeTokens();
    if (!tokens) {
      return this.respondError(id, {
        code: "internal",
        message: "theme is not initialised yet",
        retryable: true,
      });
    }
    this.respondOk(id, tokens);
  }

  /// Add the context only the shell knows (which card is rendering this frame).
  private decorate(method: string, data: unknown): unknown {
    if (method === "host.info" && data && typeof data === "object") {
      return { ...(data as Record<string, unknown>), cardId: this.cardId };
    }
    return data;
  }

  private respondOk(id: string, result: unknown): void {
    this.post({ v: UI_API_VERSION, id, ok: true, result: result ?? null });
  }

  private respondError(id: string, error: BridgeApiError | null): void {
    this.post({
      v: UI_API_VERSION,
      id,
      ok: false,
      error: error ?? { code: "internal", message: "unknown error", retryable: false },
    });
  }

  private post(message: unknown): void {
    if (this.sealed) return;
    const target = this.frame.contentWindow;
    if (!target) return;
    // P1-1: pin the origin once we know it, so a navigated (cross-origin)
    // window can never receive a reply. Opaque "null" origins cannot be
    // named — for those "*" plus the navigation seal is the protection.
    target.postMessage(JSON.stringify(message), this.pinnedOrigin ?? "*");
  }

  private clearTimer(id: string): void {
    const timer = this.timers.get(id);
    if (timer !== undefined) window.clearTimeout(timer);
    this.timers.delete(id);
  }

  private diagnostic(message: string): void {
    this.onDiagnostic?.(message);
  }
}
