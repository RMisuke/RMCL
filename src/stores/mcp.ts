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

  async function refreshStatus(): Promise<void> {
    const res = await mcp.status();
    if (!res.ok) return;
    // Skip the signal write when nothing changed (perf audit 2.9): a fresh
    // object identity per poll forced every subscriber to re-render every 2s.
    setStatus((prev) =>
      prev && JSON.stringify(prev) === JSON.stringify(res.data) ? prev : res.data
    );
  }

  async function refreshConfirmations(): Promise<void> {
    const res = await mcp.confirmations();
    if (res.ok) setConfirmations(res.data);
    await refreshRecent();
    syncPolling();
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
    const res = await mcp.resolveConfirmation(id, allow);
    if (res.ok) {
      setConfirmations((list) => list.filter((c) => c.id !== id));
      await refreshRecent();
    } else {
      // The server may have ruled already (e.g. timeout → deny): resync
      // instead of dropping the card on blind faith (P2-10).
      await refreshConfirmations();
    }
  }

  // Perf audit 3.4: the 2s poll used to run forever from app start. Events
  // (`MCP_SESSION` / `MCP_CONFIRMATION`) already drive updates, so polling is
  // only a safety net for the server-side confirmation countdown — run it
  // strictly while a confirmation is pending, and never otherwise.
  function ensurePolling(): void {
    if (pollTimer) return;
    pollTimer = setInterval(() => {
      if (confirmations().length > 0) void refreshConfirmations();
    }, 2000);
  }

  function syncPolling(): void {
    if (confirmations().length > 0) {
      ensurePolling();
    } else if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function attachEvents(onExternalConnect?: (client: string) => void): void {
    // Initial snapshot replaces the permanent poll.
    void refreshStatus();
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
    void listen<McpStatusView>(EVENTS.MCP_SESSION, () => syncPolling());
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
    refreshStatus,
    refreshTools,
    refreshConfirmations,
    refreshRecent,
    setEnabled,
    resolve,
    attachEvents,
  };
});
