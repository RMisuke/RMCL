import { describe, expect, it } from "vitest";
import {
  buildAdaptivePalette,
  contrastRatio,
  extractClusters,
  extractClustersFromFrames,
  fallbackPalette,
  paletteFromThemeAccent,
  hexToRgb,
  labToLch,
  lchToLab,
  rgbToLch,
  type Rgb,
} from "./adaptivePalette";

const block = (colors: Rgb[], size = 48): Uint8ClampedArray => {
  const data = new Uint8ClampedArray(size * size * 4);
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const index = (y * size + x) * 4;
      const color = colors[Math.min(colors.length - 1, Math.floor(x / (size / colors.length)))];
      data[index] = color[0];
      data[index + 1] = color[1];
      data[index + 2] = color[2];
      data[index + 3] = 255;
    }
  }
  return data;
};

describe("adaptive palette", () => {
  it("extracts distinct clusters from a colourful frame", () => {
    const clusters = extractClusters(
      block([
        [42, 92, 200],
        [56, 174, 156],
        [228, 148, 62],
      ]),
      48,
      48,
      6
    );
    expect(clusters.length).toBeGreaterThanOrEqual(2);
    const hues = clusters.map((cluster) => labToLch(cluster.lab).h);
    expect(new Set(hues.map((h) => Math.round(h / 30))).size).toBeGreaterThanOrEqual(2);
  });

  it("merges multiple frames and prefers the prevalent representative color", () => {
    const dominant = block([
      [42, 92, 200],
      [42, 92, 200],
    ]);
    const accent = block([
      [228, 118, 52],
      [228, 118, 52],
    ]);
    const clusters = extractClustersFromFrames(
      [
        { pixels: dominant, width: 48, height: 48, weight: 1.8 },
        { pixels: accent, width: 48, height: 48, weight: 0.45 },
      ],
      6
    );
    const palette = buildAdaptivePalette(clusters, "video", {
      mode: "dark",
      frameCount: 2,
      sampleCount: 2_304,
    });
    expect(palette.representative.primary).toBeTruthy();
    const primary = rgbToLch(hexToRgb(palette.representative.primary)!);
    const accentLch = rgbToLch([228, 118, 52]);
    // The blue frame is larger, so the resulting primary should stay cool.
    expect(Math.abs(primary.h - accentLch.h)).toBeGreaterThan(45);
  });

  it("keeps every critical pair above WCAG 4.5 in dark and light modes", () => {
    for (const mode of ["dark", "light"] as const) {
      for (const colors of [
        [
          [42, 92, 200],
          [56, 174, 156],
          [228, 148, 62],
        ],
        [
          [240, 200, 40],
          [250, 235, 180],
          [120, 90, 60],
        ],
        [
          [180, 40, 80],
          [80, 60, 160],
          [240, 190, 210],
        ],
      ] as Rgb[][]) {
        const palette = buildAdaptivePalette(extractClusters(block(colors), 48, 48, 6), "video", {
          mode,
          frameCount: 4,
          sampleCount: 9_216,
        });
        expect(palette.mode).toBe(mode);
        expect(palette.readable, `${mode}: ${palette.colors.join(",")}`).toBe(true);
        for (const [name, value] of Object.entries(palette.checks)) {
          expect(value, `${mode} ${name} in ${palette.colors.join(",")}`).toBeGreaterThanOrEqual(4.5);
        }
        expect(palette.colors).toHaveLength(3);
        expect(palette.confidence).toBeGreaterThan(0.5);
      }
    }
  });

  it("keeps the theme accent's hue when falling back from the current theme", () => {
    const palette = paletteFromThemeAccent("#b0442b", "light");
    expect(palette.readable).toBe(true);
    const accent = rgbToLch(hexToRgb(palette.colors[0])!);
    const source = rgbToLch([176, 68, 43]);
    expect(Math.abs(accent.h - source.h)).toBeLessThan(18);
  });

  it("builds a readable fallback without any sampled media", () => {
    for (const mode of ["dark", "light"] as const) {
      const palette = fallbackPalette("default", mode);
      expect(palette.readable).toBe(true);
      expect(palette.colors).toHaveLength(3);
      for (const [name, value] of Object.entries(palette.checks)) {
        expect(value, `${mode} ${name}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps the generated state coherent in both modes", () => {
    const clusters = [
      { lab: lchToLab({ L: 0.6, C: 0.14, h: 250 }), weight: 1, coverage: 0.5 },
      { lab: lchToLab({ L: 0.65, C: 0.12, h: 170 }), weight: 0.8, coverage: 0.3 },
      { lab: lchToLab({ L: 0.72, C: 0.1, h: 60 }), weight: 0.5, coverage: 0.2 },
    ];
    const dark = buildAdaptivePalette(clusters, "image", { mode: "dark" });
    const light = buildAdaptivePalette(clusters, "image", { mode: "light" });
    const darkPanel = hexToRgb(dark.vars["--adaptive-panel"])!;
    const darkInk = hexToRgb(dark.vars["--adaptive-ink"])!;
    const lightPanel = hexToRgb(light.vars["--adaptive-panel"])!;
    const lightInk = hexToRgb(light.vars["--adaptive-ink"])!;
    expect(contrastRatio(darkInk, darkPanel)).toBeGreaterThan(10);
    expect(contrastRatio(lightInk, lightPanel)).toBeGreaterThan(10);
    expect(rgbToLch(darkPanel).L).toBeLessThan(0.2);
    expect(rgbToLch(lightPanel).L).toBeGreaterThan(0.94);
    expect(dark.mode).toBe("dark");
    expect(light.mode).toBe("light");
  });
});
