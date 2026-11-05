/**
 * Single backdrop layer (ui-render-refactor S3 / `01_TARGET_ARCHITECTURE.md` P1).
 *
 * Exactly one full-screen backdrop is mounted at a time. Priority:
 *   user image > user video > theme app-scope background > ambient.
 *
 * The chosen kind is published to the render-tier engine so glass surfaces can
 * pick `full` vs `acrylic` before the first paint of a new backdrop.
 */
import type { JSX } from "solid-js";
import { Match, Switch, createEffect, createMemo } from "solid-js";
import { backgroundImage } from "../lib/backgroundImage";
import { backgroundVideo } from "../lib/backgroundVideo";
import { settingsStore } from "../stores/settings";
import { themeSkin } from "../theme";
import { setBackdropKind, type BackdropKind } from "../render/tier";
import { AmbientBackdrop } from "./AmbientBackdrop";
import { ImageBackdrop } from "./ImageBackdrop";
import { VideoBackdrop } from "./VideoBackdrop";
import { BackgroundMedia } from "../components/ThemeSurface";

type ActiveBackdrop = "user-image" | "user-video" | "theme" | "ambient";

export function BackdropLayer(): JSX.Element {
  const themeBackground = createMemo(() => {
    const skin = themeSkin();
    if (!skin?.background) return null;
    const scope = skin.background.scope;
    return scope === "app" || scope === "both" ? skin.background : null;
  });

  const active = createMemo<ActiveBackdrop>(() => {
    if (backgroundImage().enabled && backgroundImage().path) return "user-image";
    if (backgroundVideo().enabled && backgroundVideo().path && settingsStore.motionEnabled()) {
      return "user-video";
    }
    if (themeBackground()) return "theme";
    return "ambient";
  });

  const kind = createMemo<BackdropKind>(() => {
    switch (active()) {
      case "user-image":
        return "image";
      case "user-video":
        return "video";
      case "theme": {
        const bg = themeBackground();
        if (bg?.video && settingsStore.motionEnabled()) return "video";
        if (bg?.image || bg?.poster) return "image";
        return "ambient";
      }
      default:
        return "ambient";
    }
  });

  createEffect(() => setBackdropKind(kind()));

  return (
    <Switch fallback={<AmbientBackdrop />}>
      <Match when={active() === "user-image"}>
        <ImageBackdrop />
      </Match>
      <Match when={active() === "user-video"}>
        <VideoBackdrop />
      </Match>
      <Match when={themeBackground()}>
        {(bg) => <BackgroundMedia background={bg()} />}
      </Match>
    </Switch>
  );
}
