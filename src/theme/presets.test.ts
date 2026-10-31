import { describe, expect, it } from "vitest";
import type { ThemeTokens } from "../ipc/types";
import { sceneFor, visualPaletteFor } from "./presets";

const dark: ThemeTokens = {
  background: "#0b0c0f",
  surface: "#13151a",
  surfaceElevated: "#1a1d24",
  text: "#f2eee7",
  textMuted: "#a39d92",
  accent: "#efb35e",
  accentContrast: "#1b1204",
  border: "#2b2e36",
};

const light: ThemeTokens = {
  background: "#f1ece2",
  surface: "#fbf7ef",
  surfaceElevated: "#fffdf7",
  text: "#1a1713",
  textMuted: "#786f63",
  accent: "#b0442b",
  accentContrast: "#fff9f2",
  border: "#ded5c6",
};

describe("Noctis theme presets", () => {
  it("maps every built-in key to its own atmosphere and particle motif", () => {
    const keys = [
      "builtin:dark",
      "builtin:forest",
      "builtin:ocean",
      "builtin:ember",
      "builtin:light",
      "builtin:sakura",
      "builtin:paper",
      "builtin:sky",
    ];
    const scenes = keys.map((key) => sceneFor(key, key.includes("light") || ["builtin:sakura", "builtin:paper", "builtin:sky"].includes(key) ? light : dark));
    expect(new Set(scenes.map((scene) => scene.atmosphere)).size).toBeGreaterThanOrEqual(6);
    expect(new Set(scenes.map((scene) => scene.particle)).size).toBeGreaterThanOrEqual(5);
    for (const scene of scenes) {
      expect(scene.primary).toMatch(/^#[0-9a-f]{6}$/i);
      expect(scene.secondary).not.toBe(scene.primary);
      expect(scene.tertiary).not.toBe(scene.secondary);
    }
  });

  it("derives a deterministic scene for plugin themes", () => {
    const first = sceneFor("example/plugin-theme", light, "light");
    const second = sceneFor("example/plugin-theme", light, "light");
    expect(first).toEqual(second);
    expect(first.atmosphere).toBe("custom");
    expect(first.primary).toBe(light.accent);
    expect(first.particleCount).toBeGreaterThan(0);
  });

  it("falls back to the built-in light/dark scene for system preferences", () => {
    expect(sceneFor(null, dark, "dark").key).toBe("builtin:dark");
    expect(sceneFor("system", light, "light").key).toBe("builtin:light");
    expect(visualPaletteFor(null, dark, "dark").primary).toBe(dark.accent);
  });
});
