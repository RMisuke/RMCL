/**
 * Tauri event name constants — mirrors src-tauri/src/events.rs
 */

export const EVENTS = {
  // Download
  DOWNLOAD_PROGRESS: "download://progress",
  DOWNLOAD_COMPLETE: "download://complete",
  DOWNLOAD_ERROR: "download://error",
  DOWNLOAD_PAUSED: "download://paused",
  DOWNLOAD_RESUMED: "download://resumed",

  // Auth
  AUTH_STATE_CHANGED: "auth://state-changed",
  AUTH_DEVICE_CODE: "auth://device-code",

  // Launch
  LAUNCH_STARTED: "launch://started",
  LAUNCH_OUTPUT: "launch://output",
  LAUNCH_EXITED: "launch://exited",

  // Mod Loader
  MODLOADER_INSTALL_PROGRESS: "modloader://progress",

  // Instances
  INSTANCES_REFRESHED: "instances://refreshed",
} as const;
