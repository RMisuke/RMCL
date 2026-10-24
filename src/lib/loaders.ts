/**
 * Mod-loader display names, shared by the instance loader page, the install
 * wizard, and the task center (raw backend ids like `neo_forge` → NeoForge).
 */
export const LOADER_CARDS: { id: string; label: string }[] = [
  { id: "fabric", label: "Fabric" },
  { id: "neo_forge", label: "NeoForge" },
  { id: "forge", label: "Forge" },
  { id: "quilt", label: "Quilt" },
  { id: "opti_fine", label: "OptiFine" },
  { id: "lite_loader", label: "LiteLoader" },
  { id: "cleanroom", label: "Cleanroom" },
  { id: "legacy_fabric", label: "LegacyFabric" },
];

export function loaderLabel(id: string | null | undefined): string {
  return LOADER_CARDS.find((l) => l.id === id)?.label ?? (id ?? "");
}
