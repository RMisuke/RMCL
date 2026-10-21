import type { BridgeMethod, ThemeTokens, WireErrorCode } from "@rmccc/plugin-api";

export declare class HostError extends Error {
  readonly code: WireErrorCode;
  readonly retryable: boolean;
  constructor(code: string, message: string, retryable: boolean);
}

export interface ConnectOptions {
  /** Per-request timeout; the host enforces its own limits too. */
  requestTimeoutMs?: number;
}

export interface HostContext {
  v: number;
  type: "init";
  pluginId: string;
  cardId?: string;
  theme: ThemeTokens;
  limits: { maxRequestBytes: number; maxResponseBytes: number; timeoutMs: number };
}

export interface PluginHost {
  readonly apiVersion: number;
  readonly context: HostContext | null;
  onInit(handler: (context: HostContext) => void): () => void;
  onTheme(handler: (theme: ThemeTokens) => void): () => void;
  onRevoke(handler: (capability: string) => void): () => void;
  request<T = unknown>(method: BridgeMethod, params?: unknown): Promise<T>;
  info(): Promise<unknown>;
  theme(): Promise<ThemeTokens>;
  settings(): Promise<unknown>;
  instances(): Promise<unknown>;
  network(params: { url: string; method?: "GET" | "POST"; body?: string }): Promise<unknown>;
  dispose(): void;
}

export declare function connect(options?: ConnectOptions): PluginHost;
export declare function applyTheme(theme: ThemeTokens, target?: HTMLElement): void;
export declare function announce(): void;
