/**
 * Adaptive palette v2 (Noctis).
 *
 * Turns a representative sample of the visible wallpaper into a readable shell
 * palette. The sampler (adaptiveTheme.ts) now feeds several frames/regions; this
 * module merges their colour clusters, selects roles by prevalence × salience
 * × hue separation, and derives either a dark or a light surface stack.
 *
 * The hard rule is unchanged: every critical text/background pair is nudged in
 * OKLCH until it clears WCAG 4.5:1. Hues remain sampled, but the UI roles are
 * host-owned so a busy frame can never make the shell unreadable.
 */

export type Rgb = [number, number, number];
export type Lab = { L: number; a: number; b: number };
export type Lch = { L: number; C: number; h: number };
export type AdaptiveSource = "video" | "image" | "gradient" | "default";
export type AdaptiveMode = "dark" | "light";

export interface ContrastChecks {
  inkOnPanel: number;
  mutedOnPanel: number;
  accentOnPanel: number;
  whiteOnAccent: number;
  whiteOnDock: number;
  whiteOnLaunch: number;
  dockActiveInk: number;
}

export interface RepresentativeColors {
  primary: string;
  secondary: string;
  tertiary: string;
}

export interface AdaptivePalette {
  version: 2;
  source: AdaptiveSource;
  mode: AdaptiveMode;
  /** Three visible swatches (adjusted to the UI-safe palette). */
  colors: string[];
  representative: RepresentativeColors;
  vars: Record<string, string>;
  checks: ContrastChecks;
  readable: boolean;
  /** Number of video/image frames actually analysed. */
  frames: number;
  /** Number of pixel samples analysed. */
  samples: number;
  /** 0–1, how much confidence the UI may show. */
  confidence: number;
}

export interface FrameSample {
  pixels: Uint8ClampedArray;
  width: number;
  height: number;
  /** Frame importance (full frame = 1, crops may be lower). */
  weight?: number;
}

export interface AdaptiveOptions {
  mode?: AdaptiveMode;
  frameCount?: number;
  sampleCount?: number;
}

const WHITE: Rgb = [255, 255, 255];
const DARK_INK: Rgb = [10, 12, 16];
const LIGHT_INK: Rgb = [255, 250, 242];

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
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
  return 0.2126 * srgbToLinear(rgb[0]) + 0.7152 * srgbToLinear(rgb[1]) + 0.0722 * srgbToLinear(rgb[2]);
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

function ensureContrast(color: Lch, background: Rgb, target: number, prefer: "darker" | "lighter"): Lch {
  let L = color.L;
  const step = prefer === "darker" ? -0.008 : 0.008;
  for (let i = 0; i < 100; i += 1) {
    if (contrastRatio(lchToRgb({ ...color, L }), background) >= target) break;
    L += step;
    if (L <= 0.04 || L >= 0.995) break;
  }
  return { ...color, L: clamp01(L) };
}

function hueDistance(a: number, b: number): number {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
}

function distinctHue(hue: number, used: number[], amount: number): number {
  let candidate = hue;
  for (let i = 0; i < 16; i += 1) {
    if (used.every((h) => hueDistance(candidate, h) >= 30)) break;
    candidate = (candidate + amount + 360) % 360;
  }
  return candidate;
}

export interface RawCluster {
  lab: Lab;
  weight: number;
  /** Share of all sampled weight, 0–1. */
  coverage?: number;
}

interface WeightedSample {
  lab: Lab;
  weight: number;
  frame: number;
}

/**
 * Weighted k-means in OKLab over one or more downscaled frames.
 *
 * Sample weight combines:
 *  - alpha gate;
 *  - chroma (a costume/sky is more theme-worthy than a white wall);
 *  - centre bias (the subject usually lives near the middle);
 *  - frame weight (full frame > crop);
 *  - a mild rejection of near-black/near-white extremes.
 */
export function extractClustersFromFrames(frames: FrameSample[], k = 6): RawCluster[] {
  const samples: WeightedSample[] = [];
  frames.forEach((frame, frameIndex) => {
    const { pixels, width, height } = frame;
    if (!width || !height || pixels.length < width * height * 4) return;
    const frameWeight = frame.weight ?? 1;
    for (let y = 0; y < height; y += 2) {
      for (let x = 0; x < width; x += 2) {
        const index = (y * width + x) * 4;
        if (pixels[index + 3] < 128) continue;
        const lab = rgbToOklab([pixels[index], pixels[index + 1], pixels[index + 2]]);
        const lch = labToLch(lab);
        const nx = (x / Math.max(1, width - 1) - 0.5) * 2;
        const ny = (y / Math.max(1, height - 1) - 0.5) * 2;
        const center = 1 - Math.min(1, Math.hypot(nx, ny) / 1.4142);
        const extreme = lch.L < 0.07 || lch.L > 0.96 ? 0.45 : 1;
        const weight =
          frameWeight *
          (0.55 + Math.min(0.22, lch.C) * 4.2 + center * 0.55) *
          (0.78 + (1 - Math.abs(lch.L - 0.56) / 0.56) * 0.32) *
          extreme;
        samples.push({ lab, weight, frame: frameIndex });
      }
    }
  });
  if (samples.length === 0) return [];

  const vivid = samples.filter((sample) => {
    const lch = labToLch(sample.lab);
    return lch.C >= 0.018 && lch.L > 0.05 && lch.L < 0.97;
  });
  const pool = vivid.length >= Math.max(24, k * 6) ? vivid : samples;
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
      const score = sample.weight * (nearest + 0.025);
      if (score > bestScore) {
        bestScore = score;
        best = sample;
      }
    }
    centers.push(best.lab);
  }

  for (let iteration = 0; iteration < 14; iteration += 1) {
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
      moved += Math.hypot(
        next.L - centers[index].L,
        next.a - centers[index].a,
        next.b - centers[index].b
      );
      centers[index] = next;
    });
    if (moved < 0.0005) break;
  }

  const clusters = assignWeights(centers, pool);
  const merged: RawCluster[] = [];
  for (const cluster of clusters.sort((a, b) => b.weight - a.weight)) {
    const duplicate = merged.some(
      (existing) =>
        Math.hypot(
          cluster.lab.L - existing.lab.L,
          cluster.lab.a - existing.lab.a,
          cluster.lab.b - existing.lab.b
        ) < 0.055
    );
    if (!duplicate) merged.push(cluster);
    if (merged.length >= k) break;
  }
  const total = merged.reduce((sum, cluster) => sum + cluster.weight, 0) || 1;
  return merged
    .filter((cluster) => cluster.weight > 0)
    .map((cluster) => ({ ...cluster, coverage: cluster.weight / total }));
}

function assignWeights(centers: Lab[], pool: WeightedSample[]): RawCluster[] {
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

/** Compatibility wrapper for single-frame callers/tests. */
export function extractClusters(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  k = 3
): RawCluster[] {
  return extractClustersFromFrames([{ pixels, width, height, weight: 1 }], k);
}

const DEFAULT_HUES = [38, 12, 194];

function fallbackClusters(): RawCluster[] {
  return DEFAULT_HUES.map((h, index) => ({
    lab: lchToLab({ L: [0.64, 0.58, 0.7][index], C: [0.15, 0.12, 0.1][index], h }),
    weight: 1 - index * 0.2,
    coverage: [0.5, 0.3, 0.2][index],
  }));
}

function clusterScore(cluster: RawCluster): number {
  const lch = labToLch(cluster.lab);
  const coverage = Math.max(0.01, cluster.coverage ?? cluster.weight);
  const chroma = clamp(lch.C / 0.22, 0, 1);
  const balance = 0.72 + (1 - Math.abs(lch.L - 0.56) / 0.56) * 0.28;
  return Math.pow(coverage, 0.72) * (0.42 + chroma * 0.68) * balance;
}

function selectRoles(clusters: RawCluster[]): { primary: Lch; secondary: Lch; tertiary: Lch; coverage: number } {
  const usable = clusters.filter((cluster) => cluster.weight > 0);
  const fallback = fallbackClusters();
  if (usable.length === 0) usable.push(...fallback);
  const byScore = [...usable].sort((a, b) => clusterScore(b) - clusterScore(a));
  const chromatic = byScore.filter((cluster) => labToLch(cluster.lab).C >= 0.032);
  const primaryCluster = chromatic[0] ?? byScore[0];
  const primary = labToLch(primaryCluster.lab);

  const secondaryPool = byScore.filter(
    (cluster) => cluster !== primaryCluster && hueDistance(labToLch(cluster.lab).h, primary.h) >= 30
  );
  const secondaryCluster = secondaryPool[0];
  const secondary = secondaryCluster
    ? labToLch(secondaryCluster.lab)
    : {
        L: primary.L,
        C: primary.C * 0.82,
        h: distinctHue(primary.h, [primary.h], 38),
      };

  const tertiaryPool = byScore.filter((cluster) => {
    const lch = labToLch(cluster.lab);
    return (
      cluster !== primaryCluster &&
      cluster !== secondaryCluster &&
      hueDistance(lch.h, primary.h) >= 30 &&
      hueDistance(lch.h, secondary.h) >= 30
    );
  });
  const tertiaryCluster = tertiaryPool[0];
  const tertiary = tertiaryCluster
    ? labToLch(tertiaryCluster.lab)
    : {
        L: primary.L,
        C: primary.C * 0.74,
        h: distinctHue(primary.h, [primary.h, secondary.h], -54),
      };

  return {
    primary,
    secondary,
    tertiary,
    coverage: primaryCluster.coverage ?? 0.5,
  };
}

function accentFor(
  role: Lch,
  mode: AdaptiveMode,
  panel: Rgb,
  chromaMin: number,
  chromaMax: number
): Lch {
  let accent: Lch = {
    L: mode === "dark" ? 0.74 : 0.47,
    // A representative wallpaper hue should read as a deliberate accent, not
    // as the same muddy average the thumbnail already shows. Add a small,
    // bounded saturation lift while keeping the sampled hue.
    C: clamp(role.C * 1.18 + 0.015, chromaMin, chromaMax),
    h: role.h,
  };
  // Guarantee that the accent itself is distinguishable from the panel, and
  // then pick the more readable ink for text placed on it.
  for (let i = 0; i < 80; i += 1) {
    const color = lchToRgb(accent);
    const chosenInk = contrastRatio(color, DARK_INK) >= contrastRatio(color, LIGHT_INK) ? DARK_INK : LIGHT_INK;
    if (contrastRatio(color, panel) >= 4.5 && contrastRatio(color, chosenInk) >= 4.5) break;
    accent = { ...accent, L: clamp(accent.L + (mode === "dark" ? 0.006 : -0.006), 0.16, 0.9) };
  }
  return accent;
}

function contrastInk(accent: Rgb): Rgb {
  return contrastRatio(accent, DARK_INK) >= contrastRatio(accent, LIGHT_INK) ? DARK_INK : LIGHT_INK;
}

export function buildAdaptivePalette(
  clusters: RawCluster[],
  source: AdaptiveSource,
  options: AdaptiveOptions = {}
): AdaptivePalette {
  const mode = options.mode ?? "dark";
  const roles = selectRoles(clusters.length > 0 ? clusters : fallbackClusters());
  const isDark = mode === "dark";

  const canvas1 = lchToRgb({
    L: isDark ? 0.105 : 0.95,
    C: isDark ? 0.018 : 0.018,
    h: roles.primary.h,
  });
  const canvas2 = lchToRgb({
    L: isDark ? 0.085 : 0.93,
    C: isDark ? 0.016 : 0.022,
    h: roles.secondary.h,
  });
  const panel = lchToRgb({
    L: isDark ? 0.155 : 0.985,
    C: isDark ? 0.014 : 0.012,
    h: roles.primary.h,
  });
  const raised = lchToRgb({
    L: isDark ? 0.205 : 0.998,
    C: isDark ? 0.012 : 0.008,
    h: roles.primary.h,
  });
  const line = lchToRgb({
    L: isDark ? 0.29 : 0.88,
    C: isDark ? 0.02 : 0.016,
    h: roles.primary.h,
  });
  const chip = lchToRgb({
    L: isDark ? 0.19 : 0.958,
    C: isDark ? 0.022 : 0.018,
    h: roles.secondary.h,
  });
  const chipHover = lchToRgb({
    L: isDark ? 0.24 : 0.93,
    C: isDark ? 0.03 : 0.026,
    h: roles.secondary.h,
  });

  const ink = lchToRgb({
    L: isDark ? 0.94 : 0.18,
    C: isDark ? 0.008 : 0.012,
    h: roles.primary.h,
  });
  let muted: Lch = { L: isDark ? 0.68 : 0.45, C: 0.012, h: roles.primary.h };
  muted = ensureContrast(muted, panel, 4.6, isDark ? "lighter" : "darker");
  const mutedRgb = lchToRgb(muted);

  const accentLch = accentFor(roles.primary, mode, panel, isDark ? 0.12 : 0.1, isDark ? 0.2 : 0.18);
  const accent2Lch = accentFor(roles.secondary, mode, panel, isDark ? 0.09 : 0.08, isDark ? 0.18 : 0.16);
  const accent3Lch = accentFor(roles.tertiary, mode, panel, isDark ? 0.07 : 0.06, isDark ? 0.16 : 0.14);
  const accent = lchToRgb(accentLch);
  const accent2 = lchToRgb(accent2Lch);
  const accent3 = lchToRgb(accent3Lch);
  const accentContrast = contrastInk(accent);
  const accentSoft = lchToRgb({
    L: isDark ? 0.24 : 0.93,
    C: isDark ? 0.035 : 0.032,
    h: roles.primary.h,
  });
  const highlight = accent3;
  const badgeInk = contrastInk(highlight);

  // Chrome is always a dark ink slab: it gives the new shell a stable anchor
  // independent of whether the page area is light or dark.
  const dock1 = lchToRgb({
    L: 0.17,
    C: clamp(roles.primary.C * 0.18, 0.01, 0.04),
    h: roles.primary.h,
  });
  const dock2 = lchToRgb({
    L: 0.105,
    C: clamp(roles.secondary.C * 0.14, 0.008, 0.035),
    h: roles.secondary.h,
  });
  const dockInk = lchToRgb(ensureContrast({ L: 0.96, C: 0.006, h: roles.primary.h }, dock2, 7, "lighter"));
  const dockMuted = lchToRgb(
    ensureContrast({ L: 0.7, C: 0.01, h: roles.primary.h }, dock2, 4.6, "lighter")
  );
  const dockActiveInk = contrastInk(accent);

  const glow1 = lchToRgb({ L: isDark ? 0.7 : 0.6, C: 0.12, h: roles.primary.h });
  const glow2 = lchToRgb({ L: isDark ? 0.74 : 0.64, C: 0.1, h: roles.secondary.h });

  const checks: ContrastChecks = {
    inkOnPanel: contrastRatio(ink, panel),
    mutedOnPanel: contrastRatio(mutedRgb, panel),
    accentOnPanel: contrastRatio(accent, panel),
    whiteOnAccent: contrastRatio(accentContrast, accent),
    whiteOnDock: Math.min(contrastRatio(dockInk, dock1), contrastRatio(dockInk, dock2)),
    whiteOnLaunch: contrastRatio(accentContrast, accent),
    dockActiveInk: contrastRatio(dockActiveInk, accent),
  };
  const readable = Object.values(checks).every((value) => value >= 4.5);

  const frameCount = options.frameCount ?? 0;
  const sampleCount = options.sampleCount ?? 0;
  const distinctHues = new Set(
    [roles.primary.h, roles.secondary.h, roles.tertiary.h].map((hue) => Math.round(hue / 24))
  ).size;
  const confidence = clamp01(
    (frameCount > 0 ? Math.min(1, 0.45 + frameCount * 0.12) : 0.55) *
      (0.75 + Math.min(0.25, (options.sampleCount ?? 0) / 80_000)) *
      (distinctHues >= 2 ? 1 : 0.82)
  );

  const vars: Record<string, string> = {
    "--adaptive-canvas-1": rgbToHex(canvas1),
    "--adaptive-canvas-2": rgbToHex(canvas2),
    "--adaptive-panel": rgbToHex(panel),
    "--adaptive-panel-rgb": rgbCss(panel),
    "--adaptive-raised": rgbToHex(raised),
    "--adaptive-raised-rgb": rgbCss(raised),
    "--adaptive-card-rgb": rgbCss(raised),
    "--adaptive-ink": rgbToHex(ink),
    "--adaptive-muted": rgbToHex(mutedRgb),
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
    "--adaptive-dock-active": rgbToHex(accent),
    "--adaptive-dock-active-rgb": rgbCss(accent),
    "--adaptive-dock-active-ink": rgbToHex(dockActiveInk),
    "--adaptive-launch-1": rgbToHex(accent),
    "--adaptive-launch-2": rgbToHex(accent2),
    "--adaptive-launch-1-rgb": rgbCss(accent),
    "--adaptive-launch-2-rgb": rgbCss(accent2),
    "--adaptive-launch-dock-1": rgbToHex(accent),
    "--adaptive-launch-dock-2": rgbToHex(accent2),
    "--adaptive-launch-dock-ink": rgbToHex(accentContrast),
    "--adaptive-launch-dock-muted": rgbToHex(accentContrast),
    "--adaptive-ring-bg": `rgba(${rgbCss(raised)}, .82)`,
    "--adaptive-progress-bg": rgbToHex(chip),
    "--adaptive-progress-fill": rgbToHex(accent2),
    "--adaptive-badge": rgbToHex(highlight),
    "--adaptive-badge-ink": rgbToHex(badgeInk),
    "--adaptive-icon": rgbToHex(accent),
    "--adaptive-icon-muted": rgbToHex(mutedRgb),
  };

  return {
    version: 2,
    source,
    mode,
    colors: [rgbToHex(accent), rgbToHex(accent2), rgbToHex(accent3)],
    representative: {
      primary: rgbToHex(accent),
      secondary: rgbToHex(accent2),
      tertiary: rgbToHex(accent3),
    },
    vars,
    checks,
    readable,
    frames: frameCount,
    samples: sampleCount,
    confidence,
  };
}

export function fallbackPalette(source: AdaptiveSource = "default", mode: AdaptiveMode = "dark"): AdaptivePalette {
  return buildAdaptivePalette(fallbackClusters(), source, { mode });
}

export function paletteFromThemeAccent(
  accentHex: string | null | undefined,
  mode: AdaptiveMode = "dark"
): AdaptivePalette {
  const rgb = accentHex ? hexToRgb(accentHex) : null;
  if (!rgb) return fallbackPalette("default", mode);
  const lch = rgbToLch(rgb);
  const clusters: RawCluster[] = [
    {
      lab: lchToLab({ L: lch.L, C: Math.max(lch.C, 0.12), h: lch.h }),
      weight: 1,
      coverage: 0.52,
    },
    {
      lab: lchToLab({ L: 0.64, C: 0.12, h: (lch.h + 38) % 360 }),
      weight: 0.7,
      coverage: 0.3,
    },
    {
      lab: lchToLab({ L: 0.6, C: 0.09, h: (lch.h - 56 + 360) % 360 }),
      weight: 0.5,
      coverage: 0.18,
    },
  ];
  return buildAdaptivePalette(clusters, "default", { mode });
}
