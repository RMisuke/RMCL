/**
 * Pure render-tier decision (no DOM, no IPC) — kept separate so it can be
 * unit-tested in isolation. `tier.ts` adds the reactive shell around it.
 */
export type RenderTier = "full" | "acrylic" | "static";
export type BackdropKind = "ambient" | "image" | "video" | "theme";
export type PlatformKind = "windows" | "macos" | "other";
export type GpuTier = "low" | "standard" | "high";

export interface TierInputs {
  platform: PlatformKind;
  reducedMotion: boolean;
  userMotion: boolean;
  gpuTier: GpuTier;
  backdropKind: BackdropKind;
}

/**
 * Map environment to a render tier.
 *
 * Precedence matters: reduced/user motion wins over everything, then low-end
 * GPU, then a moving backdrop (never sampled by glass), then macOS (conservative
 * acrylic), then a Windows machine with a discrete GPU and a static backdrop
 * (full glass). Integrated-GPU Windows machines stay on acrylic: the full tier
 * runs every live `backdrop-filter`, and that path has not been profiled on
 * iGPUs yet.
 */
export function decideTier(input: TierInputs): RenderTier {
  if (!input.userMotion || input.reducedMotion) return "static";
  if (input.gpuTier === "low") return "static";
  if (input.backdropKind === "video") return "acrylic";
  if (input.platform === "macos") return "acrylic";
  if (
    input.platform === "windows" &&
    input.gpuTier === "high" &&
    (input.backdropKind === "ambient" || input.backdropKind === "image")
  ) {
    return "full";
  }
  return "acrylic";
}
