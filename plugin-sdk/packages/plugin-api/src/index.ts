/**
 * `@rmccc/plugin-api` — the wire contract of the isolated plugin UI bridge.
 *
 * Source of truth: `crates/rmccc-plugin/src/ui.rs` (`BRIDGE_METHODS`,
 * `BridgeErrorCode`, `UI_API_VERSION`) and `schemas/plugin-ui-bridge.schema.json`.
 * Keep this file in sync when either changes; `pnpm --dir plugin-sdk test`
 * compares the method table against the JSON schema.
 */

/** UI API major version. Bumped only on breaking changes. */
export const UI_API_VERSION = 1;

/** Request/response size and concurrency limits enforced by the host. */
export const LIMITS = {
  maxRequestBytes: 64 * 1024,
  maxResponseBytes: 512 * 1024,
  maxInFlight: 8,
  defaultTimeoutMs: 10_000,
  maxTimeoutMs: 30_000,
} as const;

/** Wire error codes (stable strings — see ADR-006). */
export type WireErrorCode =
  | "validation"
  | "unsupported_method"
  | "protocol_version"
  | "too_large"
  | "timeout"
  | "cancelled"
  | "not_installed"
  | "not_enabled"
  | "permission_denied"
  | "permission_pending"
  | "host_not_allowed"
  | "internal";

/** Bridge method names in UI API v1. */
export type BridgeMethod =
  | "host.info"
  | "theme.get"
  | "settings.get"
  | "instances.list"
  | "network.request";

/** Capability required per method (`null` = any enabled plugin with a UI entry). */
export const METHOD_CAPABILITIES: Record<BridgeMethod, string | null> = {
  "host.info": null,
  "theme.get": null,
  "settings.get": "settings.read",
  "instances.list": "instance.read",
  "network.request": "network.request",
};

/** Per-method host timeout in milliseconds. */
export const METHOD_TIMEOUTS: Record<BridgeMethod, number> = {
  "host.info": 5_000,
  "theme.get": 5_000,
  "settings.get": 10_000,
  "instances.list": 10_000,
  "network.request": 30_000,
};

/** Plugin → host request. */
export interface BridgeRequest {
  v: typeof UI_API_VERSION;
  id: string;
  method: BridgeMethod;
  params?: unknown;
}

/** Plugin → host "I am alive, send me the init payload". */
export interface ReadyMessage {
  v: typeof UI_API_VERSION;
  type: "ready";
}

/** Host → plugin responses and pushes. */
export type HostMessage =
  | {
      v: typeof UI_API_VERSION;
      id: string;
      ok: true;
      result: unknown;
    }
  | {
      v: typeof UI_API_VERSION;
      id: string;
      ok: false;
      error: WireError;
    }
  | {
      v: typeof UI_API_VERSION;
      type: "init";
      pluginId: string;
      cardId?: string;
      theme: ThemeTokens;
      limits: { maxRequestBytes: number; maxResponseBytes: number; timeoutMs: number };
    }
  | { v: typeof UI_API_VERSION; type: "theme"; theme: ThemeTokens }
  | { v: typeof UI_API_VERSION; type: "revoke"; capability: string };

/** Wire error body. */
export interface WireError {
  code: WireErrorCode;
  message: string;
  retryable?: boolean;
}

/** Dashboard palette handed to plugins (read-only; host owns the values). */
export interface ThemeTokens {
  background: string;
  surface: string;
  surfaceElevated: string;
  text: string;
  textMuted: string;
  accent: string;
  accentContrast: string;
  border: string;
}

/** Result payloads of the v1 methods. */
export interface HostInfo {
  uiApiVersion: number;
  appVersion: string;
  pluginId: string;
  cardId?: string;
  platform: string;
}

export interface SettingsSubset {
  locale: string | null;
  theme: string | null;
  defaultMinMemoryMb: number;
  defaultMaxMemoryMb: number;
}

export interface InstanceSummary {
  id: string;
  name: string;
  gameVersion: string;
  loader: { type: string; version: string } | null;
  memoryMb: number;
  lastPlayed: string | null;
}

export interface NetworkReply {
  status: number;
  body: string;
}

/** Thrown by `request()` when the host answers with an error. */
export class BridgeError extends Error {
  readonly code: WireErrorCode;
  readonly retryable: boolean;

  constructor(error: WireError) {
    super(error.message);
    this.name = "BridgeError";
    this.code = error.code;
    this.retryable = error.retryable ?? false;
  }
}
