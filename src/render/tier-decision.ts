/**
 * Pure render-tier decision (no DOM, no IPC) — kept separate so it can be
 * unit-tested in isolation. `tier.ts` adds the reactive shell around it.
 */
export type RenderTier = "full" | "acrylic" | "static";
export type BackdropKind = "ambient" | "image" | "video" | "theme";
export type PlatformKind = "windows" | "macos" | "other";
export type GpuTier = "low" | "standard";

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
 * acrylic), then Windows with a static backdrop (full glass).
 */
export function decideTier(input: TierInputs): RenderTier {
  if (!input.userMotion || input.reducedMotion) return "static";
  if (input.gpuTier === "low") return "static";
  if (input.backdropKind === "video") return "acrylic";
  if (input.platform === "macos") return "acrylic";
  if (
    input.platform === "windows" &&
    (input.backdropKind === "ambient" || input.backdropKind === "image")
  ) {
    return "full";
  }
  return "acrylic";
}
