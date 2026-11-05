import { describe, expect, it } from "vitest";
import { decideTier, type TierInputs } from "./tier-decision";

const base: TierInputs = {
  platform: "windows",
  reducedMotion: false,
  userMotion: true,
  gpuTier: "standard",
  backdropKind: "ambient",
};

describe("decideTier", () => {
  it("returns static when the user turns motion off", () => {
    expect(decideTier({ ...base, userMotion: false })).toBe("static");
  });

  it("returns static when the OS requests reduced motion", () => {
    expect(decideTier({ ...base, reducedMotion: true })).toBe("static");
  });

  it("treats reduced motion as higher priority than a low-end GPU", () => {
    expect(
      decideTier({ ...base, reducedMotion: true, gpuTier: "low" })
    ).toBe("static");
  });

  it("returns static on a low-end GPU", () => {
    expect(decideTier({ ...base, gpuTier: "low" })).toBe("static");
  });

  it("never uses full glass over a moving video backdrop", () => {
    expect(decideTier({ ...base, backdropKind: "video" })).toBe("acrylic");
  });

  it("stays acrylic on macOS even with a static backdrop", () => {
    expect(decideTier({ ...base, platform: "macos" })).toBe("acrylic");
  });

  it("keeps integrated-GPU Windows on acrylic", () => {
    expect(decideTier({ ...base, gpuTier: "standard" })).toBe("acrylic");
  });

  it("allows full glass on a discrete-GPU Windows machine with an ambient backdrop", () => {
    expect(decideTier({ ...base, gpuTier: "high", backdropKind: "ambient" })).toBe("full");
  });

  it("allows full glass on a discrete-GPU Windows machine with a still-image backdrop", () => {
    expect(decideTier({ ...base, gpuTier: "high", backdropKind: "image" })).toBe("full");
  });

  it("never uses full glass on a discrete GPU over video", () => {
    expect(decideTier({ ...base, gpuTier: "high", backdropKind: "video" })).toBe("acrylic");
  });

  it("falls back to acrylic for unknown platforms", () => {
    expect(decideTier({ ...base, platform: "other" })).toBe("acrylic");
  });
});
