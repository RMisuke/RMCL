/**
 * Adaptive palette worker (ui-render-refactor C2).
 *
 * Clustering over a downsampled frame is pure CPU work. Running it here keeps
 * the main thread free during wallpaper changes. The worker receives plain
 * `FrameSample` objects with transferable pixel buffers and replies with the
 * finished `AdaptivePalette`.
 */
import {
  buildAdaptivePalette,
  extractClustersFromFrames,
  type AdaptiveMode,
  type AdaptivePalette,
  type AdaptiveSource,
  type FrameSample,
} from "./adaptivePalette";

interface ComputeRequest {
  id: number;
  frames: FrameSample[];
  source: AdaptiveSource;
  options: { mode: AdaptiveMode; frameCount: number; sampleCount: number };
}

self.onmessage = (event: MessageEvent<ComputeRequest>) => {
  const { id, frames, source, options } = event.data;
  try {
    const clusters = extractClustersFromFrames(frames, 6);
    const palette: AdaptivePalette = buildAdaptivePalette(clusters, source, options);
    (self as unknown as Worker).postMessage({ id, palette, ready: clusters.length > 0 });
  } catch {
    (self as unknown as Worker).postMessage({ id, palette: null, ready: false });
  }
};
