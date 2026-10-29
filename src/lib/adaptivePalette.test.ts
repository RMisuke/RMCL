import { describe, expect, it } from "vitest";
import {
  buildAdaptivePalette,
  contrastRatio,
  extractClusters,
  fallbackPalette,
  hexToRgb,
  labToLch,
  lchToLab,
  rgbToLch,
  type Rgb,
} from "./adaptivePalette";

const block = (colors: Rgb[]): Uint8ClampedArray => {
  const size = 48;
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
      48
    );
    expect(clusters.length).toBeGreaterThanOrEqual(2);
    const hues = clusters.map((cluster) => labToLch(cluster.lab).h);
    expect(new Set(hues.map((h) => Math.round(h / 30))).size).toBeGreaterThanOrEqual(2);
  });

  it("keeps every critical pair above WCAG 4.5", () => {
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
      const palette = buildAdaptivePalette(extractClusters(block(colors), 48, 48), "video");
      expect(palette.readable).toBe(true);
      for (const [name, value] of Object.entries(palette.checks)) {
        expect(value, `${name} in ${palette.colors.join(",")}`).toBeGreaterThanOrEqual(4.5);
      }
      expect(palette.colors).toHaveLength(3);
    }
  });

  it("builds a readable fallback without any sampled media", () => {
    const palette = fallbackPalette();
    expect(palette.readable).toBe(true);
    expect(palette.colors).toHaveLength(3);
    for (const [name, value] of Object.entries(palette.checks)) {
      expect(value, name).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("keeps the generated state coherent (panel/ink/dock)", () => {
    const palette = buildAdaptivePalette(
      [
        { lab: lchToLab({ L: 0.6, C: 0.14, h: 250 }), weight: 1 },
        { lab: lchToLab({ L: 0.65, C: 0.12, h: 170 }), weight: 0.8 },
        { lab: lchToLab({ L: 0.72, C: 0.1, h: 60 }), weight: 0.5 },
      ],
      "image"
    );
    const panel = hexToRgb(palette.vars["--adaptive-panel"])!;
    const ink = hexToRgb(palette.vars["--adaptive-ink"])!;
    const dock = hexToRgb(palette.vars["--adaptive-dock-1"])!;
    expect(contrastRatio(ink, panel)).toBeGreaterThan(10);
    expect(contrastRatio(hexToRgb("#ffffff")!, dock)).toBeGreaterThan(4.5);
    expect(rgbToLch(panel).L).toBeGreaterThan(0.94);
  });
});
