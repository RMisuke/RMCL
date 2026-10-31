/**
 * HMCL-style instance folder shortcuts.
 *
 * HMCL exposes a single "浏览" popup containing the ten directories a player
 * actually edits (run dir, mods, resource packs, saves, schematics, shader
 * packs, screenshots, config, logs, crash reports). Each entry opens the
 * folder directly — there is no copy-path step. Keep this list and the Rust
 * `instance_reveal` match arms in sync.
 */
import type { IconName } from "../components/icons";
import type { TranslationKey } from "../i18n/locales/messages";

export type InstanceFolder =
  | "run"
  | "instance"
  | "game"
  | "mods"
  | "resourcepacks"
  | "saves"
  | "schematics"
  | "shaderpacks"
  | "screenshots"
  | "config"
  | "logs"
  | "crash-reports";

export interface InstanceFolderItem {
  id: InstanceFolder;
  labelKey: TranslationKey;
  icon: IconName;
}

export const INSTANCE_FOLDERS: InstanceFolderItem[] = [
  { id: "run", labelKey: "instanceDetail.folders.run", icon: "gamepad" },
  { id: "mods", labelKey: "instanceDetail.folders.mods", icon: "extension" },
  { id: "resourcepacks", labelKey: "instanceDetail.folders.resourcepacks", icon: "package" },
  { id: "saves", labelKey: "instanceDetail.folders.saves", icon: "hard-drive" },
  { id: "schematics", labelKey: "instanceDetail.folders.schematics", icon: "grid" },
  { id: "shaderpacks", labelKey: "instanceDetail.folders.shaderpacks", icon: "sparkles" },
  { id: "screenshots", labelKey: "instanceDetail.folders.screenshots", icon: "eye" },
  { id: "config", labelKey: "instanceDetail.folders.config", icon: "sliders" },
  { id: "logs", labelKey: "instanceDetail.folders.logs", icon: "terminal" },
  { id: "crash-reports", labelKey: "instanceDetail.folders.crashReports", icon: "alert" },
];
