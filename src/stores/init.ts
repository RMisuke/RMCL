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
import { instancesStore } from "./instances";
import { launchStore } from "./launch";
import { tasksStore } from "./tasks";
import { aiStore } from "./ai";
import { mcpStore } from "./mcp";
import { uiStore, toast } from "./ui";
import * as versionCache from "./versionCache";

let started = false;

export function initStores(): void {
  if (started) return;
  started = true;

  // ── Domain event subscriptions (each store also attaches its own listeners;
  //    the ones below cover attribution/feedback that crosses domains) ──
  authStore.attachEvents();
  instancesStore.attachEvents();
  launchStore.attachEvents();
  tasksStore.attachEvents();
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
  void listen<{
    key?: string;
    profile_id?: string;
    run_id?: string;
    exit_code: number;
    exit_type?: string;
  }>(EVENTS.LAUNCH_EXITED, (event) => {
    const profileId = event.payload.key ?? event.payload.profile_id ?? "";
    // User-initiated stops (P1-7) are not crashes — don't pop the log drawer.
    if (event.payload.exit_code !== 0 && !launchStore.consumeUserKill(profileId)) {
      // Abnormal exit: open the log drawer with the crash highlighted (10 §4).
      uiStore.openLogDrawer(profileId);
      // 崩溃自动分析（设置 · AI，默认关）：仅在已批准外发且已配置时发起。
      if (
        aiStore.autoAnalyzeOnCrash() &&
        aiStore.config()?.configured &&
        aiStore.egressApproved()
      ) {
        void aiStore
          .analyzeLog(
            profileId,
            event.payload.run_id ?? launchStore.currentRunId(profileId),
            { exitCode: event.payload.exit_code, exitType: event.payload.exit_type ?? "Abnormal" }
          )
          .then((res) => {
            if (!res.ok && res.message) toast({ message: res.message, tone: "error" });
          });
      }
    }
    // The backend persists this session's play time while handling the exit;
    // refresh shortly after so the account/launch docks pick it up without a
    // page reload (the list command itself is cheap — no completion scan).
    setTimeout(() => void instancesStore.refresh(true), 1200);
  });
  // ── Startup data ──
  void authStore.refresh();
  void instancesStore.refresh();
  void launchStore.restoreStatus();
  void aiStore.refreshConfig();
  void mcpStore.refreshStatus();
  void mcpStore.refreshConfirmations();
  // 游戏版本 manifest：启动时后台刷新一次（versionCache 供下载向导/实例页秒开）。
  void versionCache.refreshGameVersions();
}
