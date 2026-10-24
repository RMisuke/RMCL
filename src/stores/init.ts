/**
 * Central store bootstrap: registers EVERY Tauri event subscription exactly
 * once (ui-design 00 §5.2 rule: stores are the only listen() point) and
 * performs the startup data load.
 *
 * Called from App on mount; idempotent.
 */
import { listen } from "@tauri-apps/api/event";
import { EVENTS } from "../ipc/events";
import { t } from "../i18n";
import { dispatchPluginPush } from "../plugin-ui/bridge";
import { authStore } from "./auth";
import { profilesStore } from "./profiles";
import { launchStore } from "./launch";
import { downloadsStore } from "./downloads";
import { modloadersStore } from "./modloaders";
import { aiStore } from "./ai";
import { mcpStore } from "./mcp";
import { uiStore, toast } from "./ui";

let started = false;

export function initStores(): void {
  if (started) return;
  started = true;

  // ── Domain event subscriptions (each store also attaches its own listeners;
  //    the ones below cover attribution/feedback that crosses domains) ──
  authStore.attachEvents();
  profilesStore.attachEvents();
  launchStore.attachEvents();
  downloadsStore.attachEvents();
  modloadersStore.attachEvents();
  aiStore.attachEvents();
  mcpStore.attachEvents((client) => {
    toast({ message: t("tasks.agentConnected", { client }), tone: "info" });
  });

  // ── Plugin push relay (S6 llm.chat streaming) ──
  void listen<{ pluginId: string; message: unknown }>(EVENTS.PLUGIN_PUSH, (event) => {
    dispatchPluginPush(event.payload.pluginId, event.payload.message);
  });

  // ── Attribution toasts: external Agent operations (mcp://call) ──
  void listen<{ kind: string; sessionId: string; tool: string; ok: boolean }>(
    EVENTS.MCP_CALL,
    (event) => {
      if (event.payload.kind === "started") {
        toast({ message: t("tasks.agentStarted", { tool: event.payload.tool }), tone: "info" });
      } else if (!event.payload.ok) {
        toast({ message: t("tasks.agentFailed", { tool: event.payload.tool }), tone: "error" });
      }
      void mcpStore.refreshRecent();
    }
  );

  // ── Cross-domain reactions ──
  void listen<{ profile_id: string; exit_code: number }>(EVENTS.LAUNCH_EXITED, (event) => {
    if (event.payload.exit_code !== 0) {
      // Abnormal exit: open the log drawer with the crash highlighted (10 §4).
      uiStore.openLogDrawer(event.payload.profile_id);
    }
  });
  void listen<{ profile_id: string; version: string }>(EVENTS.DOWNLOAD_COMPLETE, () => {
    void profilesStore.refresh(true);
  });

  // ── Startup data ──
  void authStore.refresh();
  void profilesStore.refresh();
  void launchStore.restoreStatus();
  void aiStore.refreshConfig();
  void mcpStore.refreshStatus();
  void mcpStore.refreshConfirmations();
}
