/**
 * 彩色图标资源（icon resource/ 入库到 src/assets/mc-icons/）。
 *
 * Vite 会把 import 的 png 哈希打包；引用一律走本模块，不要再手写路径。
 * 显示用 `.mc-icon` / `.loader-card-icon` / `.toast-bell` 等 class：
 * 固定宽高 + object-fit: contain，按比例缩放不失真。
 */
import bell from "../assets/mc-icons/bell-128.png";
import grassBlock from "../assets/mc-icons/grass_block-128.png";
import commandBlock from "../assets/mc-icons/command_block-128.png";
import cobblestone from "../assets/mc-icons/cobblestone-128.png";
import fabric from "../assets/mc-icons/fabric.png";
import neoForge from "../assets/mc-icons/NeoForge.png";
import forge from "../assets/mc-icons/forge.png";
import quilt from "../assets/mc-icons/Quilt.png";
import liteLoader from "../assets/mc-icons/LiteLoader.png";
import cleanroom from "../assets/mc-icons/Cleanroom.png";
import legacyFabric from "../assets/mc-icons/LegacyFabric.png";

/** 通知气泡图标（最左侧）。 */
export const BELL_ICON = bell;

/** 版本类型 → 图标：正式版草方块 / 快照命令方块 / 远古版圆石。 */
export function versionTypeIcon(releaseType: string): string {
  if (releaseType === "snapshot") return commandBlock;
  if (releaseType === "old_alpha" || releaseType === "old_beta") return cobblestone;
  return grassBlock;
}

/** Mod loader id → 图标（OptiFine 用命令方块）。 */
export const LOADER_ICONS: Record<string, string> = {
  fabric,
  neo_forge: neoForge,
  forge,
  quilt,
  opti_fine: commandBlock,
  lite_loader: liteLoader,
  cleanroom,
  legacy_fabric: legacyFabric,
};

export function loaderIcon(id: string | null | undefined): string | null {
  return (id && LOADER_ICONS[id]) || null;
}
