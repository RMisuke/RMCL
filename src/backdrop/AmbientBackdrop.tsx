/**
 * Ambient backdrop (ui-render-refactor S3 / `03_BACKGROUND_SUBSYSTEM.md` §4).
 *
 * A purely static backdrop baked from theme tokens + the host scene preset.
 * Unlike the previous `AmbientStage`, this layer:
 *  - has no `filter: blur()` veil and no full-screen `mix-blend-mode`;
 *  - renders particles only as a static texture (never animation);
 *  - drops the particle DOM entirely on the `static` tier.
 *
 * Visibility is decided by `BackdropLayer`; this component always renders its
 * own complete frame when mounted.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo } from "solid-js";
import { themeScene } from "../theme";
import { themeV4 } from "../theme/v4/store";
import { renderTier } from "../render/tier";
import type { ThemeScene } from "../theme/presets";

/** Deterministic pseudo-random so a reactive re-render never reshuffles. */
function seeded(index: number, salt: number): number {
  const value = Math.sin(index * 127.1 + salt * 311.7) * 43758.5453;
  return value - Math.floor(value);
}

const FALLBACK_SCENE: ThemeScene = {
  key: "builtin:dark",
  label: "Noctis",
  atmosphere: "noctis",
  primary: "#efb35e",
  secondary: "#d96b4f",
  tertiary: "#73c9d4",
  particle: "mote",
  particleCount: 30,
  particleSpeed: 28,
  particleOpacity: 0.6,
};

export function AmbientBackdrop(): JSX.Element {
  const scene = createMemo(() => themeScene() ?? FALLBACK_SCENE);
  const count = createMemo(() => {
    // `static` tier renders no particle DOM at all; the gradients carry the look.
    if (renderTier() === "static") return 0;
    const width = typeof window === "undefined" ? 1440 : window.innerWidth;
    const density = Math.min(1.35, Math.max(0.72, width / 1440));
    return Math.max(10, Math.round(scene().particleCount * 0.55 * density));
  });

  return (
    <div
      class="ambient-stage"
      data-atmosphere={scene().atmosphere}
      data-particle={scene().particle}
      aria-hidden="true"
    >
      <Show when={themeV4.motionId()}>
        {(motion) => (
          <div class="theme-motion" data-motion-theme={motion()} aria-hidden="true">
            <i class="tm-a" />
            <i class="tm-b" />
            <i class="tm-c" />
          </div>
        )}
      </Show>
      <div class="ambient-wash" />
      <div class="ambient-veil">
        <i class="is-a" />
        <i class="is-b" />
        <i class="is-c" />
      </div>
      <div class="ambient-horizon" />
      <div class="ambient-grid" />
      <div class="ambient-particles">
        <For each={Array.from({ length: count() })}>
          {(_, index) => {
            const i = index();
            const depth = seeded(i, 2);
            return (
              <span
                class={`ambient-particle is-${scene().particle}`}
                style={{
                  left: `${4 + seeded(i, 11) * 92}%`,
                  top: `${6 + seeded(i, 12) * 88}%`,
                  "--particle-size": `${(i % 9 === 0 ? 6 + depth * 4 : 2.2 + depth * 4.5).toFixed(2)}px`,
                  "--particle-color":
                    i % 7 === 0
                      ? "var(--scene-tertiary)"
                      : i % 3 === 0
                        ? "var(--scene-secondary)"
                        : "var(--scene-primary)",
                }}
              />
            );
          }}
        </For>
      </div>
      <div class="ambient-vignette" />
      <div class="ambient-grain" />
    </div>
  );
}
