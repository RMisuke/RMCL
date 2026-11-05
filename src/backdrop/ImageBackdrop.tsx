/**
 * Still-image wallpaper backdrop (ui-render-refactor S3).
 *
 * One static layer; decoding and scaling stay in the system WebView. No canvas,
 * no animation, no resident work. The dim scrim is a static `::after` overlay.
 */
import type { JSX } from "solid-js";
import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import {
  backgroundImage,
  primeBackgroundImage,
  setBackgroundImageIssue,
} from "../lib/backgroundImage";
import { scheduleAdaptiveSample } from "../lib/adaptiveTheme";

export function ImageBackdrop(): JSX.Element {
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

  onCleanup(() => {
    generation++;
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
