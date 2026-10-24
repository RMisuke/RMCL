/**
 * MCP store (ADR-013 S4/S5): server status, sessions, tool policies,
 * pending confirmations + recent calls. Events `mcp://session/confirmation`
 * drive updates; confirmations also poll (2s) while any are pending because
 * the countdown resolution (timeout → deny) happens server-side.
 */
import { createRoot, createSignal } from "solid-js";
import { mcp } from "../ipc/api";
import type { McpStatusView, McpToolView, McpConfirmationView, McpRecentCallView } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

let pollTimer: ReturnType<typeof setInterval> | null = null;

export const mcpStore = createRoot(() => {
  const [status, setStatus] = createSignal<McpStatusView | null>(null);
  const [tools, setTools] = createSignal<McpToolView[]>([]);
  const [confirmations, setConfirmations] = createSignal<McpConfirmationView[]>([]);
  const [recentCalls, setRecentCalls] = createSignal<McpRecentCallView[]>([]);
  const [connectedToastsDone, setConnectedToastsDone] = createSignal(false);

  async function refreshStatus(): Promise<void> {
    const res = await mcp.status();
    if (res.ok) setStatus(res.data);
  }

  async function refreshConfirmations(): Promise<void> {
    const res = await mcp.confirmations();
    if (res.ok) setConfirmations(res.data);
    await refreshRecent();
  }

  async function refreshRecent(): Promise<void> {
    const res = await mcp.recentCalls(20);
    if (res.ok) setRecentCalls(res.data);
  }

  async function refreshTools(): Promise<void> {
    const res = await mcp.toolsList();
    if (res.ok) setTools(res.data);
  }

  async function setEnabled(enabled: boolean): Promise<void> {
    const res = await mcp.setEnabled(enabled);
    if (res.ok) setStatus(res.data);
  }

  async function resolve(id: string, allow: boolean): Promise<void> {
    await mcp.resolveConfirmation(id, allow);
    setConfirmations((list) => list.filter((c) => c.id !== id));
    await refreshRecent();
  }

  function ensurePolling(): void {
    if (pollTimer) return;
    pollTimer = setInterval(() => {
      if (confirmations().length > 0) void refreshConfirmations();
      else if (status()?.enabled) void refreshStatus();
    }, 2000);
  }

  function attachEvents(onExternalConnect?: (client: string) => void): void {
    ensurePolling();
    void listen<{ kind: string; sessionId: string; client: string }>(
      EVENTS.MCP_SESSION,
      (event) => {
        void refreshStatus();
        // Toast only for external tools (client name non-empty), not our own
        // session bookkeeping; suppress the first burst while settings open.
        if (event.payload.client) {
          if (event.payload.kind === "connected") {
            onExternalConnect?.(event.payload.client);
          }
        }
      }
    );
    void listen<{ kind: string; confirmationId: string }>(EVENTS.MCP_CONFIRMATION, () => {
      void refreshConfirmations();
    });
    void listen(EVENTS.MCP_CALL, () => {
      // Attribution events: refresh audit view lazily (settings page reads it).
      if (status()?.enabled) void refreshRecent();
    });
  }

  return {
    status,
    tools,
    confirmations,
    recentCalls,
    connectedToastsDone,
    setConnectedToastsDone,
    refreshStatus,
    refreshTools,
    refreshConfirmations,
    refreshRecent,
    setEnabled,
    resolve,
    attachEvents,
  };
});
