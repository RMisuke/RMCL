/**
 * Theme v2 visual layer (ADR-014).
 *
 * All values come from the backend-resolved skin: URLs are built and validated
 * in Rust, so this file only lays out host-owned markup. There is no
 * theme-authored CSS/JS here — effects are a fixed preset library parameterised
 * by the theme (`rotating-blocks` / `aurora` / `particles`).
 */
import { For, Match, Show, Switch } from "solid-js";
import { themeSkin } from "../theme";
import { AmbientStage } from "./AmbientStage";
import type { SkinBackground, SkinEffect } from "../ipc/types";

/** Deterministic pseudo-random so a re-render never reshuffles positions. */
function seeded(index: number, salt: number): number {
  const value = Math.sin(index * 127.1 + salt * 311.7) * 43758.5453;
  return value - Math.floor(value);
}

function BackgroundMedia(props: {
  background: SkinBackground;
  class?: string;
}) {
  const fit = () =>
    props.background.fit === "contain" ? "contain" : "cover";
  const position = () => props.background.position || "center";
  return (
    <div
      class={`theme-media ${props.class ?? ""}`}
      style={{
        "--media-fit": fit(),
        "--media-position": position(),
        ...(props.background.blur
          ? { "--media-blur": `${props.background.blur}px` }
          : {}),
      }}
      aria-hidden="true"
    >
      <Show when={props.background.poster}>
        <img
          class="theme-media-poster"
          src={props.background.poster!}
          alt=""
        />
      </Show>
      <Show when={props.background.video}>
        <video
          class="theme-media-video"
          src={props.background.video!}
          poster={props.background.poster ?? undefined}
          autoplay
          loop
          muted
          playsinline
          preload="metadata"
        />
      </Show>
      <Show when={!props.background.video && props.background.image}>
        <img
          class="theme-media-image"
          src={props.background.image!}
          alt=""
        />
      </Show>
      <span
        class="theme-media-dim"
        style={{ background: `rgba(2, 7, 12, ${props.background.dim})` }}
      />
    </div>
  );
}

function Aurora(props: { effect: SkinEffect }) {
  const intensity = () => props.effect.intensity ?? 0.5;
  const speed = () => props.effect.speed ?? 22;
  return (
    <div
      class="theme-aurora"
      style={{
        "--aurora-intensity": String(intensity()),
        "--aurora-speed": `${speed()}s`,
      }}
    >
      <i />
      <i />
      <i />
    </div>
  );
}

function RotatingBlocks(props: { effect: SkinEffect }) {
  const texture = () => props.effect.texture;
  const count = () => props.effect.count ?? 8;
  const size = () => props.effect.size ?? 56;
  const speed = () => props.effect.speed ?? 18;
  const opacity = () => props.effect.opacity ?? 0.55;
  return (
    <Show when={texture()}>
      {(src) => (
        <For each={Array.from({ length: count() })}>
          {(_, index) => {
            const i = index();
            const left = 42 + seeded(i, 1) * 40;
            const top = 6 + seeded(i, 2) * 72;
            const delay = -(seeded(i, 3) * 20);
            const duration = speed() + seeded(i, 4) * 8;
            const blockOpacity = opacity() * (0.55 + seeded(i, 5) * 0.45);
            return (
              <span
                class="theme-block"
                style={{
                  left: `${left}%`,
                  top: `${top}%`,
                  "--size": `${size()}px`,
                  "--speed": `${duration}s`,
                  "--delay": `${delay}s`,
                  opacity: String(blockOpacity),
                }}
              >
                <span class="theme-block-spin">
                  <i
                    class="face front"
                    style={{ "background-image": `url("${src()}")` }}
                  />
                  <i
                    class="face back"
                    style={{ "background-image": `url("${src()}")` }}
                  />
                  <i
                    class="face left"
                    style={{ "background-image": `url("${src()}")` }}
                  />
                  <i
                    class="face right"
                    style={{ "background-image": `url("${src()}")` }}
                  />
                  <i
                    class="face top"
                    style={{ "background-image": `url("${src()}")` }}
                  />
                </span>
              </span>
            );
          }}
        </For>
      )}
    </Show>
  );
}

function Particles(props: { effect: SkinEffect }) {
  const count = () => props.effect.count ?? 16;
  const color = () => props.effect.color ?? "var(--rmccc-accent)";
  const speed = () => props.effect.speed ?? 18;
  const shape = () => props.effect.shape ?? "dot";
  return (
    <For each={Array.from({ length: count() })}>
      {(_, index) => {
        const i = index();
        return (
          <span
            class={`theme-particle is-${shape()}`}
            style={{
              left: `${seeded(i, 11) * 100}%`,
              top: `${seeded(i, 12) * 100}%`,
              "--particle-size": `${2 + seeded(i, 13) * 4}px`,
              "--particle-color": color(),
              "--particle-duration": `${speed() + seeded(i, 14) * 10}s`,
              "--particle-delay": `${-(seeded(i, 15) * speed())}s`,
              "--particle-drift": `${(seeded(i, 16) - 0.5) * 140}px`,
            }}
          />
        );
      }}
    </For>
  );
}

function EffectView(props: { effect: SkinEffect }) {
  return (
    <Switch>
      <Match when={props.effect.kind === "aurora"}>
        <Aurora effect={props.effect} />
      </Match>
      <Match when={props.effect.kind === "rotating-blocks"}>
        <RotatingBlocks effect={props.effect} />
      </Match>
      <Match when={props.effect.kind === "particles"}>
        <Particles effect={props.effect} />
      </Match>
    </Switch>
  );
}

function ThemeEffects(props: { area: "app" | "hero" }) {
  const effects = () =>
    (themeSkin()?.effects ?? []).filter(
      (effect) => effect.area === props.area || effect.area === "both"
    );
  return (
    <Show when={effects().length > 0}>
      <div class={`theme-effects is-${props.area}`}>
        <For each={effects()}>{(effect) => <EffectView effect={effect} />}</For>
      </div>
    </Show>
  );
}

/** Full-window wallpaper + app-scope effects (rendered by the app shell). */
export function ThemeBackdrop() {
  const background = () => {
    const skin = themeSkin();
    if (!skin?.background) return null;
    const scope = skin.background.scope;
    return scope === "app" || scope === "both" ? skin.background : null;
  };
  const hostAmbient = () => !background();
  return (
    <div class="theme-backdrop" aria-hidden="true">
      <Show when={hostAmbient()}>
        <AmbientStage />
      </Show>
      <Show when={background()} keyed>
        {(bg) => <BackgroundMedia background={bg} />}
      </Show>
      <ThemeEffects area="app" />
    </div>
  );
}

/** Hero-scope wallpaper + effects (rendered inside Home's `.hero`). */
export function ThemeHeroLayer() {
  const background = () => {
    const skin = themeSkin();
    if (!skin?.background) return null;
    // `both` is served by the single app-level layer; the hero becomes
    // transparent (see theme-skin.css) so the video is decoded only once.
    const scope = skin.background.scope;
    return scope === "hero" ? skin.background : null;
  };
  return (
    <Show when={background() || themeSkin()?.effects?.length}>
      <div class="theme-hero-layer" aria-hidden="true">
        <Show when={background()} keyed>
          {(bg) => <BackgroundMedia background={bg} />}
        </Show>
        <ThemeEffects area="hero" />
      </div>
    </Show>
  );
}
