/**
 * Optional local MP4 wallpaper.
 *
 * The WebView can only read the file through Tauri's asset protocol, whose
 * scope is in-memory. On every launch we therefore ask the backend to re-grant
 * access (the command also canonicalises the path), then build the `asset://`
 * URL. The layer is mounted only while the feature is enabled and motion
 * effects are on, so an unused video never costs decode time.
 */
import type { JSX } from "solid-js";
import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { backgroundMedia } from "../ipc/api";
import {
  backgroundVideo,
  setBackgroundVideoIssue,
} from "../lib/backgroundVideo";
import { settingsStore } from "../stores/settings";
import { scheduleAdaptiveSample } from "../lib/adaptiveTheme";

export function BackgroundVideo(): JSX.Element {
  const [src, setSrc] = createSignal<string | null>(null);

  const wanted = () =>
    backgroundVideo().enabled &&
    Boolean(backgroundVideo().path) &&
    settingsStore.motionEnabled();

  createEffect(() => {
    const path = wanted() ? backgroundVideo().path : "";
    if (!path) {
      setSrc(null);
      return;
    }
    let cancelled = false;
    setSrc(null);

    void backgroundMedia.allow(path).then((res) => {
      if (cancelled) return;
      if (!res.ok) {
        console.warn("BackgroundVideo: could not read video", res.error);
        setBackgroundVideoIssue(res.error.message);
        return;
      }
      try {
        setSrc(convertFileSrc(res.data));
        setBackgroundVideoIssue("");
      } catch (e) {
        console.warn("BackgroundVideo: could not build asset URL", e);
        setBackgroundVideoIssue(String(e));
      }
    });

    onCleanup(() => {
      cancelled = true;
    });
  });

  return (
    <Show when={src()}>
      {(url) => (
        <div
          class="app-video-layer"
          aria-hidden="true"
          style={{ "--video-dim": String(backgroundVideo().dim / 100) }}
        >
          <video
            src={url()}
            autoplay
            loop
            muted
            playsinline
            preload="auto"
            onLoadedData={() => scheduleAdaptiveSample(200)}
            onError={() => {
              setSrc(null);
              setBackgroundVideoIssue("decode");
            }}
          />
        </div>
      )}
    </Show>
  );
}
