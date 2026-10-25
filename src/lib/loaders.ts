/**
 * Mod-loader display names, shared by the instance loader page, the install
 * wizard, and the task center (raw backend ids like `neo_forge` → NeoForge).
 *
 * Cleanroom / LegacyFabric 已从 UI 移除（2026-10-25 产品决定）；后端安装
 * 能力保留，仅不再展示入口。
 */
export const LOADER_CARDS: { id: string; label: string }[] = [
  { id: "fabric", label: "Fabric" },
  { id: "neo_forge", label: "NeoForge" },
  { id: "forge", label: "Forge" },
  { id: "quilt", label: "Quilt" },
  { id: "opti_fine", label: "OptiFine" },
  { id: "lite_loader", label: "LiteLoader" },
];

export function loaderLabel(id: string | null | undefined): string {
  return LOADER_CARDS.find((l) => l.id === id)?.label ?? (id ?? "");
}
