/**
 * One persistent native-resolution video wallpaper.
 *
 * The system WebView owns GPU decode and composition. Routing never touches this
 * element; there are no poster caches or forced decoder resets. Only source
 * changes and the loop boundary affect playback.
 *
 * Window occlusion is deliberately not treated as a stop signal: WKWebView can
 * release the composited video surface when a window is hidden, and pausing in
 * `visibilitychange` made the wallpaper come back blank after switching apps.
 * The element keeps playing while hidden; if WebKit did pause it, the visible
 * handler resumes (or reloads the same source) once the compositor is attached.
 * When the still-image wallpaper is active this layer is unmounted, so a hidden
 * MP4 never decodes in the background.
 */
import type { JSX } from "solid-js";
import { Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import {
  backgroundVideo,
  backgroundVideoPreparing,
  primeBackgroundVideo,
  setBackgroundVideoIssue,
} from "../lib/backgroundVideo";
import { settingsStore } from "../stores/settings";
import { backgroundImage } from "../lib/backgroundImage";
import { scheduleAdaptiveSample } from "../lib/adaptiveTheme";
import { reportFrontendMessage } from "../ipc/api";
import { t } from "../i18n";

export function BackgroundVideo(): JSX.Element {
  const [src, setSrc] = createSignal<string | null>(null);
  let video: HTMLVideoElement | undefined;
  let generation = 0;
  let renderedPath: string | null = null;
  let hiddenAt = 0;
  let playingLogged = false;
  let canvas: HTMLCanvasElement | undefined;
  let canvasCtx: CanvasRenderingContext2D | null = null;
  let drawRaf = 0;
  let lastDrawAt = 0;
  let frameCallback = 0;
  let lastPresentedAt = 0;
  let lastRecoveryAt = 0;
  let recoveryCount = 0;
  let stableFrames = 0;
  let suppressSampleUntil = 0;
  let watchdogTimer: number | undefined;

  const wanted = () =>
    backgroundVideo().enabled &&
    Boolean(backgroundVideo().path) &&
    !backgroundImage().enabled &&
    settingsStore.motionEnabled();

  function logVideo(
    event: string,
    level: "info" | "warn" | "error" = "info"
  ): void {
    const el = video;
    void reportFrontendMessage(
      `background video: ${event} wanted=${wanted()} enabled=${backgroundVideo().enabled} ` +
        `motion=${settingsStore.motionEnabled()} image=${backgroundImage().enabled} ` +
        `src=${Boolean(src())} ready=${el?.readyState ?? -1} paused=${el?.paused ?? true} ` +
        `error=${el?.error?.code ?? 0} t=${el?.currentTime?.toFixed(2) ?? "-"}`,
      level
    );
  }

  function playVideo(): void {
    const el = video;
    if (!el || !wanted()) return;
    if (el.ended) el.currentTime = 0;
    void el.play().catch((error: unknown) => {
      if (error instanceof DOMException && error.name === "AbortError") return;
      logVideo(`play failed ${String(error)}`, "warn");
    });
  }

  /**
   * WKWebView can decode this 4K wallpaper and expose frames to canvas while
   * leaving the native <video> compositor surface transparent. Paint the
   * decoded frames into a DOM canvas so the wallpaper joins the normal page
   * paint path used by every other surface.
   */
  function sizeCanvas(): void {
    if (!canvas) return;
    const dpr = Math.min(Math.max(window.devicePixelRatio || 1, 1), 1.5);
    const width = Math.max(1, Math.round(window.innerWidth * dpr));
    const height = Math.max(1, Math.round(window.innerHeight * dpr));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    canvasCtx = canvas.getContext("2d", { alpha: true });
    if (canvasCtx) canvasCtx.clearRect(0, 0, width, height);
  }

  function drawVideoFrame(force = false): void {
    const now = performance.now();
    if (!force && now - lastDrawAt < 33) return;
    lastDrawAt = now;
    const ctx = canvasCtx;
    const el = video;
    const target = canvas;
    if (!ctx || !el || !target || el.readyState < 2 || !el.videoWidth) return;
    const vw = el.videoWidth;
    const vh = el.videoHeight;
    const cw = target.width;
    const ch = target.height;
    const scale = Math.max(cw / vw, ch / vh);
    const sw = cw / scale;
    const sh = ch / scale;
    const sx = (vw - sw) / 2;
    const sy = (vh - sh) / 2;
    try {
      ctx.drawImage(el, sx, sy, sw, sh, 0, 0, cw, ch);
    } catch {
      // A frame can be unavailable for one tick; the next rAF retries.
    }
  }

  function drawLoop(): void {
    if (wanted() && document.visibilityState !== "hidden") drawVideoFrame();
    drawRaf = window.requestAnimationFrame(drawLoop);
  }

  function onResize(): void {
    sizeCanvas();
    drawVideoFrame(true);
  }

  /**
   * Track frames that were actually presented to the compositor. `currentTime`
   * can keep advancing while WKWebView has dropped the video surface, so it is
   * not a reliable liveness signal. `requestVideoFrameCallback` only fires when
   * a frame is available for compositing.
   */
  function watchFrames(): void {
    const el = video;
    if (!el || !wanted()) return;
    if (typeof el.requestVideoFrameCallback !== "function") return;
    if (frameCallback && typeof el.cancelVideoFrameCallback === "function") {
      el.cancelVideoFrameCallback(frameCallback);
      frameCallback = 0;
    }
    frameCallback = el.requestVideoFrameCallback(() => {
      lastPresentedAt = performance.now();
      stableFrames += 1;
      if (stableFrames > 30) recoveryCount = 0;
      watchFrames();
    });
  }

  /**
   * Re-attach the media pipeline without tearing down the whole wallpaper.
   * `load()` reopens the same asset and is enough for a dropped compositor
   * surface; repeated failures fall back to remounting the <video> element.
   */
  function recoverSurface(reason: string): void {
    const el = video;
    if (!el || !wanted()) return;
    const now = performance.now();
    if (now - lastRecoveryAt < 5000) return;
    lastRecoveryAt = now;
    recoveryCount += 1;
    stableFrames = 0;
    logVideo(`surface recovery (${reason}) attempt=${recoveryCount}`, "warn");

    if (recoveryCount >= 3) {
      // A hard remount creates a fresh WKWebView media layer. Keep the same
      // prepared asset URL; do not re-run video palette sampling for it.
      const current = src();
      recoveryCount = 0;
      suppressSampleUntil = performance.now() + 4000;
      if (frameCallback && typeof el.cancelVideoFrameCallback === "function") {
        el.cancelVideoFrameCallback(frameCallback);
      }
      frameCallback = 0;
      video = undefined;
      lastPresentedAt = 0;
      setSrc(null);
      window.setTimeout(() => {
        if (wanted()) setSrc(current);
      }, 120);
      return;
    }

    suppressSampleUntil = performance.now() + 4000;
    try {
      el.pause();
      el.load();
    } catch {
      // Keep the play() retry below as the fallback.
    }
    lastPresentedAt = performance.now();
    window.setTimeout(() => {
      playVideo();
    }, 180);
  }

  function checkPlayback(): void {
    if (!wanted() || document.visibilityState === "hidden" || !document.hasFocus()) {
      return;
    }
    const el = video;
    if (!el) return;
    if (el.paused) {
      playVideo();
      return;
    }
    if (el.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) {
      recoverSurface("not-ready");
      return;
    }
    if (typeof el.requestVideoFrameCallback !== "function") return;
    if (lastPresentedAt === 0) {
      lastPresentedAt = performance.now();
      watchFrames();
      return;
    }
    if (performance.now() - lastPresentedAt > 2600) {
      recoverSurface("no presented frame");
    }
  }

  function restartPlayback(): void {
    if (!video) return;
    video.currentTime = 0;
    lastPresentedAt = performance.now();
    playVideo();
  }

  /**
   * WKWebView may drop the last decoded frame while the window is occluded even
   * though `src` is intact. Reload the same asset only when the element has no
   * usable frame; otherwise a normal play() is enough.
   */
  function recoverPlayback(): void {
    const el = video;
    if (!el || !wanted()) return;
    const wasHiddenFor = hiddenAt > 0 ? performance.now() - hiddenAt : 0;
    hiddenAt = 0;
    if (el.error || el.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) {
      try {
        el.load();
      } catch {
        // The element is still attached; play() below remains the fallback.
      }
    } else if (wasHiddenFor > 800 && el.currentTime >= el.duration - 0.2) {
      el.currentTime = 0;
    }
    lastPresentedAt = performance.now();
    playVideo();
    watchFrames();
    // One delayed retry covers the frame where the compositor is re-attached.
    window.setTimeout(playVideo, 180);
  }

  function onVisibilityChange(): void {
    if (document.visibilityState === "hidden") {
      // Do not pause: keeping the media pipeline alive avoids the blank surface
      // on macOS. Only record when it happened so recovery can distinguish a
      // short focus switch from a long occlusion.
      hiddenAt = performance.now();
    } else {
      lastPresentedAt = performance.now();
      window.requestAnimationFrame(recoverPlayback);
    }
  }

  /**
   * WKWebView can suspend media when the window loses key focus without firing
   * `visibilitychange`. The app's own `focus` event is the reliable signal that
   * the compositor is interactive again, so resume there as well.
   */
  function onWindowFocus(): void {
    lastPresentedAt = performance.now();
    window.requestAnimationFrame(recoverPlayback);
  }

  createEffect(() => {
    const current = backgroundVideo();
    const active = wanted();
    const path = current.path;
    if (!active || !path) {
      if (renderedPath !== null) {
        generation++;
        renderedPath = null;
        setSrc(null);
      }
      return;
    }
    // The dim slider replaces the prefs object; only a real source change
    // should tear down and reload the video element.
    if (renderedPath === path) return;
    renderedPath = path;
    const ticket = ++generation;
    setSrc(null);
    void primeBackgroundVideo().then((asset) => {
      if (ticket !== generation) return;
      if (asset) setSrc(asset.url);
      else renderedPath = null;
    });
  });

  createEffect(() => {
    const root = document.documentElement;
    if (wanted() && src()) root.dataset.videoWallpaper = "on";
    else delete root.dataset.videoWallpaper;

    if (wanted()) playVideo();
    else video?.pause();
  });

  onMount(() => {
    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("focus", onWindowFocus);
    window.addEventListener("pageshow", onWindowFocus);
    window.addEventListener("resize", onResize);
    watchdogTimer = window.setInterval(checkPlayback, 1000);
    drawRaf = window.requestAnimationFrame(drawLoop);
    playVideo();
  });

  onCleanup(() => {
    generation++;
    if (watchdogTimer !== undefined) window.clearInterval(watchdogTimer);
    watchdogTimer = undefined;
    if (drawRaf) window.cancelAnimationFrame(drawRaf);
    drawRaf = 0;
    const el = video;
    if (frameCallback && el && typeof el.cancelVideoFrameCallback === "function") {
      el.cancelVideoFrameCallback(frameCallback);
    }
    frameCallback = 0;
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("focus", onWindowFocus);
    window.removeEventListener("pageshow", onWindowFocus);
    window.removeEventListener("resize", onResize);
    delete document.documentElement.dataset.videoWallpaper;
    video?.pause();
    video = undefined;
  });

  return (
    <>
      <Show when={backgroundVideoPreparing() && wanted()}>
        <div class="app-video-preparing" role="status">
          {t("settings.appearance.video.preparing")}
        </div>
      </Show>
      <Show when={src()}>
        {(url) => (
          <div
            class="app-video-layer"
            aria-hidden="true"
            style={{ "--video-dim": String(backgroundVideo().dim / 100) }}
          >
            <video
              ref={(el) => {
                video = el;
                el.loop = true;
              }}
              src={url()}
              loop
              muted
              autoplay
              playsinline
              preload="auto"
              controls={false}
              tabIndex={-1}
              onCanPlay={playVideo}
              onPlaying={() => {
                setBackgroundVideoIssue("");
                lastPresentedAt = performance.now();
                stableFrames = 0;
                watchFrames();
                drawVideoFrame(true);
                if (!playingLogged) {
                  playingLogged = true;
                  logVideo("playing");
                }
              }}
              onLoadedData={() => {
                setBackgroundVideoIssue("");
                if (performance.now() >= suppressSampleUntil) {
                  scheduleAdaptiveSample(250);
                }
                drawVideoFrame(true);
                playVideo();
              }}
              onEnded={restartPlayback}
              onError={() => {
                setBackgroundVideoIssue("decode");
                logVideo("element error", "error");
              }}
            />
            <canvas
              class="app-video-canvas"
              aria-hidden="true"
              ref={(el) => {
                canvas = el;
                sizeCanvas();
                drawVideoFrame(true);
              }}
            />
          </div>
        )}
      </Show>
    </>
  );
}
