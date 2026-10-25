/**
 * Tauri event name constants — mirrors src-tauri/src/events.rs
 */

export const EVENTS = {
  // Download
  DOWNLOAD_PROGRESS: "download://progress",
  DOWNLOAD_COMPLETE: "download://complete",
  DOWNLOAD_ERROR: "download://error",

  // Auth
  AUTH_DEVICE_CODE: "auth://device-code",

  // Launch
  LAUNCH_STARTED: "launch://started",
  LAUNCH_OUTPUT: "launch://output",
  LAUNCH_EXITED: "launch://exited",

  // Mod Loader
  MODLOADER_INSTALL_PROGRESS: "modloader://progress",

  // Instances
  INSTANCES_REFRESHED: "instances://refreshed",

  // AI service (ADR-013): streaming log-analysis events, keyed by analysisId
  AI_DELTA: "ai://delta",
  AI_DONE: "ai://done",
  AI_ERROR: "ai://error",

  // MCP server (ADR-013 S4/S5): sessions / confirmations / attribution
  MCP_SESSION: "mcp://session",
  MCP_CONFIRMATION: "mcp://confirmation",
  MCP_CALL: "mcp://call",

  // Host → frontend relay of host→plugin bridge pushes (S6 llm.chat streaming)
  PLUGIN_PUSH: "plugin://push",
} as const;
