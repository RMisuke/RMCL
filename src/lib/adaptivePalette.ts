/**
 * Adaptive palette: turn the dominant colours of a user wallpaper/video into a
 * readable light launcher theme.
 *
 * The hard rule is visibility: every generated text/background pair is nudged
 * in OKLCH lightness until it clears a WCAG contrast target. Extracted hues are
 * preserved, but saturation/lightness are clamped so the result still works as
 * a UI theme rather than a literal copy of a busy frame.
 */

export type Rgb = [number, number, number];
export type Lab = { L: number; a: number; b: number };
export type Lch = { L: number; C: number; h: number };
export type AdaptiveSource = "video" | "image" | "gradient" | "default";

export interface ContrastChecks {
  inkOnPanel: number;
  mutedOnPanel: number;
  accentOnPanel: number;
  whiteOnAccent: number;
  whiteOnDock: number;
  whiteOnLaunch: number;
  dockActiveInk: number;
}

export interface AdaptivePalette {
  version: 1;
  source: AdaptiveSource;
  /** Three visible swatches (adjusted to the UI-safe palette). */
  colors: string[];
  vars: Record<string, string>;
  checks: ContrastChecks;
  readable: boolean;
}

const WHITE: Rgb = [255, 255, 255];

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

const clamp01 = (value: number) => clamp(value, 0, 1);

function srgbToLinear(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

function linearToSrgb(value: number): number {
  const c = clamp01(value);
  const v = c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
  return Math.round(clamp01(v) * 255);
}

export function rgbToOklab(rgb: Rgb): Lab {
  const r = srgbToLinear(rgb[0]);
  const g = srgbToLinear(rgb[1]);
  const b = srgbToLinear(rgb[2]);
  const l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
  const m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
  const s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
  const l_ = Math.cbrt(l);
  const m_ = Math.cbrt(m);
  const s_ = Math.cbrt(s);
  return {
    L: 0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_,
    a: 1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_,
    b: 0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_,
  };
}

export function oklabToRgb(lab: Lab): Rgb {
  const l_ = lab.L + 0.3963377774 * lab.a + 0.2158037573 * lab.b;
  const m_ = lab.L - 0.1055613458 * lab.a - 0.0638541728 * lab.b;
  const s_ = lab.L - 0.0894841775 * lab.a - 1.291485548 * lab.b;
  const l = l_ * l_ * l_;
  const m = m_ * m_ * m_;
  const s = s_ * s_ * s_;
  return [
    linearToSrgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    linearToSrgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    linearToSrgb(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

export function labToLch(lab: Lab): Lch {
  const C = Math.hypot(lab.a, lab.b);
  let h = (Math.atan2(lab.b, lab.a) * 180) / Math.PI;
  if (h < 0) h += 360;
  return { L: lab.L, C, h };
}

export function lchToLab(lch: Lch): Lab {
  const rad = (lch.h * Math.PI) / 180;
  return { L: lch.L, a: lch.C * Math.cos(rad), b: lch.C * Math.sin(rad) };
}

export function lchToRgb(lch: Lch): Rgb {
  return oklabToRgb(lchToLab({ ...lch, L: clamp01(lch.L), C: Math.max(0, lch.C) }));
}

export function rgbToLch(rgb: Rgb): Lch {
  return labToLch(rgbToOklab(rgb));
}

export function rgbToHex(rgb: Rgb): string {
  return `#${rgb.map((c) => clamp(Math.round(c), 0, 255).toString(16).padStart(2, "0")).join("")}`;
}

export function hexToRgb(hex: string): Rgb | null {
  const value = hex.trim().replace(/^#/, "");
  if (!/^[0-9a-f]{6}$/i.test(value)) return null;
  return [
    parseInt(value.slice(0, 2), 16),
    parseInt(value.slice(2, 4), 16),
    parseInt(value.slice(4, 6), 16),
  ];
}

export function relativeLuminance(rgb: Rgb): number {
  return (
    0.2126 * srgbToLinear(rgb[0]) +
    0.7152 * srgbToLinear(rgb[1]) +
    0.0722 * srgbToLinear(rgb[2])
  );
}

export function contrastRatio(a: Rgb, b: Rgb): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const hi = Math.max(la, lb);
  const lo = Math.min(la, lb);
  return (hi + 0.05) / (lo + 0.05);
}

function rgbCss(rgb: Rgb): string {
  return `${rgb[0]}, ${rgb[1]}, ${rgb[2]}`;
}

function ensureContrast(
  color: Lch,
  background: Rgb,
  target: number,
  prefer: "darker" | "lighter"
): Lch {
  let L = color.L;
  const step = prefer === "darker" ? -0.01 : 0.01;
  for (let i = 0; i < 70; i += 1) {
    if (contrastRatio(lchToRgb({ ...color, L }), background) >= target) break;
    L += step;
    if (L <= 0.1 || L >= 0.99) break;
  }
  return { ...color, L: clamp01(L) };
}

function ensureWhiteContrast(color: Lch, target: number): Lch {
  return ensureContrast(color, WHITE, target, "darker");
}

function ensureDarkContrast(color: Lch, background: Rgb, target: number): Lch {
  return ensureContrast(color, background, target, "darker");
}

function hueDistance(a: number, b: number): number {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
}

function distinctHue(hue: number, used: number[], amount: number): number {
  let candidate = hue;
  for (let i = 0; i < 12; i += 1) {
    if (used.every((h) => hueDistance(candidate, h) >= 28)) break;
    candidate = (candidate + amount + 360) % 360;
  }
  return candidate;
}

export interface RawCluster {
  lab: Lab;
  weight: number;
}

/**
 * Weighted k-means (k <= 3) in OKLab over a downscaled RGBA frame.
 * Neutral/dark/white pixels get lower weight so a sky or costume colour wins
 * over a large white wall or black hair mass.
 */
export function extractClusters(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  k = 3
): RawCluster[] {
  const samples: { lab: Lab; weight: number }[] = [];
  for (let y = 0; y < height; y += 2) {
    for (let x = 0; x < width; x += 2) {
      const index = (y * width + x) * 4;
      if (pixels[index + 3] < 128) continue;
      const lab = rgbToOklab([pixels[index], pixels[index + 1], pixels[index + 2]]);
      const lch = labToLch(lab);
      const weight = Math.max(
        0.08,
        0.25 +
          lch.C * 5.5 +
          (1 - Math.abs(lch.L - 0.58) * 1.7) * 0.9
      );
      samples.push({ lab, weight });
    }
  }
  if (samples.length === 0) return [];

  const vivid = samples.filter((sample) => {
    const c = labToLch(sample.lab).C;
    const l = sample.lab.L;
    return c >= 0.025 && l > 0.06 && l < 0.95;
  });
  const pool = vivid.length >= Math.max(12, k * 4) ? vivid : samples;
  const centers: Lab[] = [];

  let first = pool[0];
  for (const sample of pool) if (sample.weight > first.weight) first = sample;
  centers.push(first.lab);
  while (centers.length < k) {
    let best = pool[0];
    let bestScore = -1;
    for (const sample of pool) {
      const nearest = Math.min(
        ...centers.map((center) =>
          Math.hypot(sample.lab.L - center.L, sample.lab.a - center.a, sample.lab.b - center.b)
        )
      );
      const score = sample.weight * (nearest + 0.02);
      if (score > bestScore) {
        bestScore = score;
        best = sample;
      }
    }
    centers.push(best.lab);
  }

  let clusters: RawCluster[] = centers.map((lab) => ({ lab, weight: 0 }));
  for (let iteration = 0; iteration < 12; iteration += 1) {
    const accum = centers.map(() => ({ L: 0, a: 0, b: 0, weight: 0 }));
    for (const sample of pool) {
      let nearest = 0;
      let nearestDistance = Number.POSITIVE_INFINITY;
      centers.forEach((center, index) => {
        const distance = Math.hypot(
          sample.lab.L - center.L,
          sample.lab.a - center.a,
          sample.lab.b - center.b
        );
        if (distance < nearestDistance) {
          nearestDistance = distance;
          nearest = index;
        }
      });
      accum[nearest].L += sample.lab.L * sample.weight;
      accum[nearest].a += sample.lab.a * sample.weight;
      accum[nearest].b += sample.lab.b * sample.weight;
      accum[nearest].weight += sample.weight;
    }
    let moved = 0;
    accum.forEach((entry, index) => {
      if (entry.weight <= 0) return;
      const next = {
        L: entry.L / entry.weight,
        a: entry.a / entry.weight,
        b: entry.b / entry.weight,
      };
      moved += Math.hypot(next.L - centers[index].L, next.a - centers[index].a, next.b - centers[index].b);
      centers[index] = next;
    });
    if (moved < 0.0005) break;
  }
  clusters = accumToClusters(centers, pool);
  const merged: RawCluster[] = [];
  for (const cluster of clusters.sort((a, b) => b.weight - a.weight)) {
    const lab = cluster.lab;
    const duplicate = merged.some((existing) =>
      Math.hypot(lab.L - existing.lab.L, lab.a - existing.lab.a, lab.b - existing.lab.b) < 0.06
    );
    if (!duplicate) merged.push(cluster);
    if (merged.length >= k) break;
  }
  return merged.filter((cluster) => cluster.weight > 0);
}

function accumToClusters(
  centers: Lab[],
  pool: { lab: Lab; weight: number }[]
): RawCluster[] {
  const accum = centers.map(() => ({ L: 0, a: 0, b: 0, weight: 0 }));
  for (const sample of pool) {
    let nearest = 0;
    let nearestDistance = Number.POSITIVE_INFINITY;
    centers.forEach((center, index) => {
      const distance = Math.hypot(
        sample.lab.L - center.L,
        sample.lab.a - center.a,
        sample.lab.b - center.b
      );
      if (distance < nearestDistance) {
        nearestDistance = distance;
        nearest = index;
      }
    });
    accum[nearest].L += sample.lab.L * sample.weight;
    accum[nearest].a += sample.lab.a * sample.weight;
    accum[nearest].b += sample.lab.b * sample.weight;
    accum[nearest].weight += sample.weight;
  }
  return centers
    .map((center, index) =>
      accum[index].weight > 0
        ? {
            lab: {
              L: accum[index].L / accum[index].weight,
              a: accum[index].a / accum[index].weight,
              b: accum[index].b / accum[index].weight,
            },
            weight: accum[index].weight,
          }
        : { lab: center, weight: 0 }
    )
    .filter((cluster) => cluster.weight > 0);
}

const DEFAULT_HUES = [212, 158, 84];

function fallbackClusters(): RawCluster[] {
  return DEFAULT_HUES.map((h, index) => ({
    lab: lchToLab({ L: [0.58, 0.6, 0.66][index], C: [0.11, 0.1, 0.1][index], h }),
    weight: 1,
  }));
}

export function buildAdaptivePalette(
  clusters: RawCluster[],
  source: AdaptiveSource
): AdaptivePalette {
  const usable = clusters.length > 0 ? clusters : fallbackClusters();
  const lches = usable
    .slice(0, 3)
    .map((cluster) => labToLch(cluster.lab))
    .filter((lch) => Number.isFinite(lch.h) && Number.isFinite(lch.C));

  const rawHues = lches.map((lch) => lch.h);
  const primaryHue = rawHues[0] ?? DEFAULT_HUES[0];
  const secondaryHue =
    rawHues[1] !== undefined && hueDistance(rawHues[1], primaryHue) >= 24
      ? rawHues[1]
      : distinctHue(primaryHue, [primaryHue], 42);
  const tertiaryHue =
    rawHues[2] !== undefined &&
    hueDistance(rawHues[2], primaryHue) >= 24 &&
    hueDistance(rawHues[2], secondaryHue) >= 24
      ? rawHues[2]
      : distinctHue(primaryHue, [primaryHue, secondaryHue], -48);

  const rawChroma = lches.map((lch) => lch.C);
  const c1 = clamp(rawChroma[0] ?? 0.11, 0.09, 0.17);
  const c2 = clamp(rawChroma[1] ?? 0.1, 0.07, 0.15);
  const c3 = clamp(rawChroma[2] ?? 0.09, 0.05, 0.13);

  const panel = lchToRgb({ L: 0.978, C: 0.013, h: primaryHue });
  const raised = lchToRgb({ L: 0.992, C: 0.008, h: primaryHue });
  const canvas1 = lchToRgb({ L: 0.938, C: 0.02, h: primaryHue });
  const canvas2 = lchToRgb({ L: 0.957, C: 0.016, h: secondaryHue });
  const ink = lchToRgb({ L: 0.215, C: 0.016, h: primaryHue });
  let muted = lchToRgb({ L: 0.47, C: 0.014, h: primaryHue });
  let mutedLch: Lch = { L: 0.47, C: 0.014, h: primaryHue };
  mutedLch = ensureContrast(mutedLch, panel, 4.6, "darker");
  muted = lchToRgb(mutedLch);
  const line = lchToRgb({ L: 0.9, C: 0.012, h: primaryHue });
  const chip = lchToRgb({ L: 0.962, C: 0.01, h: primaryHue });
  const chipHover = lchToRgb({ L: 0.936, C: 0.022, h: secondaryHue });

  let accentLch: Lch = { L: 0.57, C: c1, h: primaryHue };
  for (let i = 0; i < 45; i += 1) {
    const color = lchToRgb(accentLch);
    if (contrastRatio(color, panel) >= 4.5 && contrastRatio(color, WHITE) >= 4.5) break;
    accentLch = { ...accentLch, L: Math.max(0.18, accentLch.L - 0.012) };
  }
  const accent = lchToRgb(accentLch);
  let accent2Lch: Lch = { L: 0.55, C: c2, h: secondaryHue };
  for (let i = 0; i < 45; i += 1) {
    const color = lchToRgb(accent2Lch);
    if (contrastRatio(color, panel) >= 4.5 && contrastRatio(color, WHITE) >= 4.5) break;
    accent2Lch = { ...accent2Lch, L: Math.max(0.2, accent2Lch.L - 0.012) };
  }
  const accent2 = lchToRgb(accent2Lch);
  const accent3 = lchToRgb(
    ensureDarkContrast({ L: 0.58, C: c3, h: tertiaryHue }, panel, 4.2)
  );
  const accentContrast = contrastRatio(accent, WHITE) >= 4.5 ? WHITE : ink;
  const accentSoft = lchToRgb({ L: 0.93, C: 0.035, h: primaryHue });
  const highlight = lchToRgb({ L: 0.82, C: 0.08, h: tertiaryHue });
  const badgeInk = lchToRgb({ L: 0.23, C: 0.018, h: primaryHue });

  const dock1 = lchToRgb(
    ensureWhiteContrast({ L: 0.28, C: 0.032, h: primaryHue }, 7)
  );
  const dock2 = lchToRgb(
    ensureWhiteContrast({ L: 0.2, C: 0.024, h: secondaryHue }, 7)
  );
  const dockInk = lchToRgb({ L: 0.97, C: 0.008, h: primaryHue });
  const dockMuted = lchToRgb(
    ensureContrast({ L: 0.74, C: 0.012, h: primaryHue }, dock2, 4.6, "lighter")
  );
  const dockActiveInk = lchToRgb(
    ensureDarkContrast({ L: 0.3, C: 0.02, h: primaryHue }, raised, 7)
  );

  let launch1Lch: Lch = { L: 0.52, C: c1, h: primaryHue };
  let launch2Lch: Lch = { L: 0.48, C: c2, h: secondaryHue };
  for (let i = 0; i < 30; i += 1) {
    const c1Color = lchToRgb(launch1Lch);
    const c2Color = lchToRgb(launch2Lch);
    if (contrastRatio(c1Color, WHITE) >= 4.5 && contrastRatio(c2Color, WHITE) >= 4.5) break;
    launch1Lch = { ...launch1Lch, L: Math.max(0.2, launch1Lch.L - 0.012) };
    launch2Lch = { ...launch2Lch, L: Math.max(0.18, launch2Lch.L - 0.012) };
  }
  const launch1 = lchToRgb(launch1Lch);
  const launch2 = lchToRgb(launch2Lch);
  const launchDock1 = lchToRgb(
    ensureWhiteContrast({ L: 0.36, C: c1, h: primaryHue }, 5)
  );
  const launchDock2 = lchToRgb(
    ensureWhiteContrast({ L: 0.31, C: c2, h: secondaryHue }, 5)
  );
  const launchDockInk = lchToRgb({ L: 0.985, C: 0.004, h: primaryHue });
  const launchDockMuted = lchToRgb({ L: 0.78, C: 0.01, h: primaryHue });
  const glow1 = lchToRgb({ L: 0.72, C: 0.11, h: primaryHue });
  const glow2 = lchToRgb({ L: 0.76, C: 0.1, h: secondaryHue });

  const checks: ContrastChecks = {
    inkOnPanel: contrastRatio(ink, panel),
    mutedOnPanel: contrastRatio(muted, panel),
    accentOnPanel: contrastRatio(accent, panel),
    whiteOnAccent: contrastRatio(accentContrast, accent),
    whiteOnDock: Math.min(contrastRatio(dockInk, dock1), contrastRatio(dockInk, dock2)),
    whiteOnLaunch: Math.min(contrastRatio(WHITE, launch1), contrastRatio(WHITE, launch2)),
    dockActiveInk: contrastRatio(dockActiveInk, raised),
  };
  const readable =
    checks.inkOnPanel >= 4.5 &&
    checks.mutedOnPanel >= 4.5 &&
    checks.accentOnPanel >= 4.5 &&
    checks.whiteOnAccent >= 4.5 &&
    checks.whiteOnDock >= 4.5 &&
    checks.whiteOnLaunch >= 4.5 &&
    checks.dockActiveInk >= 4.5;

  const vars: Record<string, string> = {
    "--adaptive-canvas-1": rgbToHex(canvas1),
    "--adaptive-canvas-2": rgbToHex(canvas2),
    "--adaptive-panel": rgbToHex(panel),
    "--adaptive-panel-rgb": rgbCss(panel),
    "--adaptive-raised": rgbToHex(raised),
    "--adaptive-raised-rgb": rgbCss(raised),
    "--adaptive-card-rgb": rgbCss(raised),
    "--adaptive-ink": rgbToHex(ink),
    "--adaptive-muted": rgbToHex(muted),
    "--adaptive-line": rgbToHex(line),
    "--adaptive-primary": rgbToHex(accent),
    "--adaptive-secondary": rgbToHex(accent2),
    "--adaptive-tertiary": rgbToHex(accent3),
    "--adaptive-accent": rgbToHex(accent),
    "--adaptive-accent-secondary": rgbToHex(accent2),
    "--adaptive-accent-tertiary": rgbToHex(accent3),
    "--adaptive-accent-contrast": rgbToHex(accentContrast),
    "--adaptive-accent-soft": rgbToHex(accentSoft),
    "--adaptive-chip": rgbToHex(chip),
    "--adaptive-chip-hover": rgbToHex(chipHover),
    "--adaptive-highlight": rgbToHex(highlight),
    "--adaptive-highlight-ink": rgbToHex(badgeInk),
    "--adaptive-glow-1": `rgba(${rgbCss(glow1)}, .16)`,
    "--adaptive-glow-2": `rgba(${rgbCss(glow2)}, .12)`,
    "--adaptive-dock-1": rgbToHex(dock1),
    "--adaptive-dock-2": rgbToHex(dock2),
    "--adaptive-dock-ink": rgbToHex(dockInk),
    "--adaptive-dock-muted": rgbToHex(dockMuted),
    "--adaptive-dock-active": rgbToHex(raised),
    "--adaptive-dock-active-rgb": rgbCss(raised),
    "--adaptive-dock-active-ink": rgbToHex(dockActiveInk),
    "--adaptive-launch-1": rgbToHex(launch1),
    "--adaptive-launch-2": rgbToHex(launch2),
    "--adaptive-launch-1-rgb": rgbCss(launch1),
    "--adaptive-launch-2-rgb": rgbCss(launch2),
    "--adaptive-launch-dock-1": rgbToHex(launchDock1),
    "--adaptive-launch-dock-2": rgbToHex(launchDock2),
    "--adaptive-launch-dock-ink": rgbToHex(launchDockInk),
    "--adaptive-launch-dock-muted": rgbToHex(launchDockMuted),
    "--adaptive-ring-bg": `rgba(${rgbCss(raised)}, .82)`,
    "--adaptive-progress-bg": rgbToHex(chip),
    "--adaptive-progress-fill": rgbToHex(accent2),
    "--adaptive-badge": rgbToHex(highlight),
    "--adaptive-badge-ink": rgbToHex(badgeInk),
    "--adaptive-icon": rgbToHex(accent),
    "--adaptive-icon-muted": rgbToHex(muted),
  };

  return {
    version: 1,
    source,
    colors: [rgbToHex(accent), rgbToHex(accent2), rgbToHex(accent3)],
    vars,
    checks,
    readable,
  };
}

export function fallbackPalette(source: AdaptiveSource = "default"): AdaptivePalette {
  return buildAdaptivePalette(fallbackClusters(), source);
}

export function paletteFromThemeAccent(accentHex: string | null | undefined): AdaptivePalette {
  const rgb = accentHex ? hexToRgb(accentHex) : null;
  if (!rgb) return fallbackPalette();
  const lch = rgbToLch(rgb);
  const clusters: RawCluster[] = [
    { lab: lchToLab({ L: lch.L, C: Math.max(lch.C, 0.09), h: lch.h }), weight: 1 },
    {
      lab: lchToLab({ L: 0.6, C: 0.1, h: (lch.h + 42) % 360 }),
      weight: 0.8,
    },
    {
      lab: lchToLab({ L: 0.64, C: 0.09, h: (lch.h - 48 + 360) % 360 }),
      weight: 0.6,
    },
  ];
  return buildAdaptivePalette(clusters, "default");
}
