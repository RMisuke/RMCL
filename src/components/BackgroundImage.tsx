/**
 * One persistent still-image wallpaper.
 *
 * The layer sits directly above the theme/video backdrop and below every
 * foreground surface. Decoding is delegated to the system WebView; the image is
 * never copied, preprocessed or uploaded. Motion settings do not hide it because
 * a still image does not animate.
 */
import type { JSX } from "solid-js";
import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import {
  backgroundImage,
  primeBackgroundImage,
  setBackgroundImageIssue,
} from "../lib/backgroundImage";
import { scheduleAdaptiveSample } from "../lib/adaptiveTheme";

export function BackgroundImage(): JSX.Element {
  const [src, setSrc] = createSignal<string | null>(null);
  let generation = 0;
  let renderedPath: string | null = null;

  createEffect(() => {
    const current = backgroundImage();
    const active = current.enabled && Boolean(current.path);
    const path = current.path;
    if (!active || !path) {
      if (renderedPath !== null) {
        generation++;
        renderedPath = null;
        setSrc(null);
      }
      return;
    }
    // Dim updates replace the whole prefs object; keep the decoded layer while
    // only the scrim changes so dragging the slider cannot flash the image.
    if (renderedPath === path) return;
    renderedPath = path;
    const ticket = ++generation;
    setSrc(null);
    void primeBackgroundImage().then((asset) => {
      if (ticket !== generation) return;
      if (asset) setSrc(asset.url);
      else renderedPath = null;
    });
  });

  createEffect(() => {
    const root = document.documentElement;
    if (src() && backgroundImage().enabled) root.dataset.imageWallpaper = "on";
    else delete root.dataset.imageWallpaper;
  });

  onCleanup(() => {
    generation++;
    delete document.documentElement.dataset.imageWallpaper;
  });

  return (
    <Show when={src()}>
      {(url) => (
        <div
          class="app-image-layer"
          aria-hidden="true"
          style={{ "--image-dim": String(backgroundImage().dim / 100) }}
        >
          <img
            src={url()}
            alt=""
            draggable={false}
            decoding="async"
            loading="eager"
            onLoad={() => scheduleAdaptiveSample(160)}
            onError={() => setBackgroundImageIssue("decode")}
          />
        </div>
      )}
    </Show>
  );
}
