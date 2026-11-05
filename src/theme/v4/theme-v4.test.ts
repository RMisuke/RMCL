import { describe, expect, it } from "vitest";
import { deriveTokens, isDarkBase, luminance, mix } from "./palette";
import { DEFAULT_DAY_NIGHT, resolvePhase } from "./daynight";
import { THEME_REGISTRY, migrateLegacyPreference, themeById } from "./registry";

describe("palette derivation", () => {
  it("derives surfaces between background and text", () => {
    const tokens = deriveTokens({ accent: "#8fb7ff", background: "#0b1020", text: "#e8ecf7" });
    expect(tokens.surface).not.toBe(tokens.background);
    expect(luminance(tokens.surface)).toBeGreaterThan(luminance(tokens.background));
    expect(luminance(tokens.surfaceElevated)).toBeGreaterThan(luminance(tokens.surface));
    expect(tokens.accentContrast).toBe("#15100a");
  });

  it("mixes towards the top color proportionally", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#000000", "#ffffff", 0)).toBe("#000000");
  });

  it("classifies dark bases", () => {
    expect(isDarkBase({ accent: "#fff", background: "#0b1020", text: "#eee" })).toBe(true);
    expect(isDarkBase({ accent: "#333", background: "#fbf4e8", text: "#222" })).toBe(false);
  });
});

describe("day/night resolution", () => {
  const at = (hours: number, minutes = 0) => new Date(2026, 10, 4, hours, minutes);

  it("off keeps the theme default", () => {
    expect(resolvePhase({ ...DEFAULT_DAY_NIGHT, mode: "off" }, "night", false, at(12))).toBe("night");
  });

  it("manual toggle overrides the default in off mode", () => {
    expect(
      resolvePhase({ ...DEFAULT_DAY_NIGHT, mode: "off", manual: "day" }, "night", true, at(12))
    ).toBe("day");
    expect(
      resolvePhase({ ...DEFAULT_DAY_NIGHT, mode: "off", manual: "night" }, "day", false, at(12))
    ).toBe("night");
  });

  it("auto follows the system scheme", () => {
    expect(resolvePhase({ ...DEFAULT_DAY_NIGHT, mode: "auto" }, "night", true, at(12))).toBe("night");
    expect(resolvePhase({ ...DEFAULT_DAY_NIGHT, mode: "auto" }, "night", false, at(0))).toBe("day");
  });

  it("schedule wraps across midnight", () => {
    const state = { mode: "schedule" as const, dayStart: "22:00", dayEnd: "06:00", manual: null };
    expect(resolvePhase(state, "night", false, at(23))).toBe("day");
    expect(resolvePhase(state, "night", false, at(3))).toBe("day");
    expect(resolvePhase(state, "night", false, at(12))).toBe("night");
  });
});

describe("registry", () => {
  it("exposes exactly eight selectable palettes", () => {
    expect(THEME_REGISTRY).toHaveLength(8);
    expect(THEME_REGISTRY.filter((theme) => theme.kind === "preset")).toHaveLength(6);
  });

  it("gives every preset a motion backdrop and dual palettes", () => {
    for (const theme of THEME_REGISTRY.filter((item) => item.kind === "preset")) {
      expect(theme.motion).toBe(theme.id);
      expect(theme.day.background).not.toBe(theme.night.background);
    }
  });

  it("maps legacy preferences onto v4 ids", () => {
    expect(migrateLegacyPreference("builtin:forest")).toBe("verdant");
    expect(migrateLegacyPreference(null)).toBe("stellar");
    expect(themeById("nope")).toBeNull();
  });
});
