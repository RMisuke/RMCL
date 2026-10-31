/**
 * Noctis ambient stage (theme v3).
 *
 * The default wallpaper when the user has not selected an image/video. It is
 * built only from the active theme tokens + host scene preset (no network, no
 * canvas, no image decode): a slow atmospheric wash, a fine perspective grid,
 * a horizon arc and a theme-specific particle motif.
 *
 * The layer is aria-hidden and pointer-events:none. `data-motion` and
 * `prefers-reduced-motion` both gate every animation.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo } from "solid-js";
import { themeScene } from "../theme";
import { backgroundImage } from "../lib/backgroundImage";
import { backgroundVideo } from "../lib/backgroundVideo";
import { settingsStore } from "../stores/settings";
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

function stageVisible(): boolean {
  if (backgroundImage().enabled) return false;
  return !(backgroundVideo().enabled && settingsStore.motionEnabled());
}

export function AmbientStage(): JSX.Element {
  const scene = createMemo(() => themeScene() ?? FALLBACK_SCENE);
  const count = createMemo(() => {
    // Keep the DOM small on 720p and slightly richer on large displays.
    const width = typeof window === "undefined" ? 1440 : window.innerWidth;
    return Math.max(16, Math.round(scene().particleCount * Math.min(1.45, Math.max(0.72, width / 1440))));
  });

  return (
    <Show when={stageVisible()}>
      <div
        class="ambient-stage"
        data-atmosphere={scene().atmosphere}
        data-particle={scene().particle}
        aria-hidden="true"
      >
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
                    "--particle-duration": `${(scene().particleSpeed * (0.7 + depth * 0.8)).toFixed(1)}s`,
                    "--particle-delay": `${(-seeded(i, 13) * scene().particleSpeed).toFixed(1)}s`,
                    "--particle-drift-x": `${((seeded(i, 14) - 0.5) * 120).toFixed(0)}px`,
                    "--particle-drift-y": `${(90 + seeded(i, 15) * 220).toFixed(0)}px`,
                    "--particle-spin": `${((seeded(i, 16) - 0.5) * 180).toFixed(0)}deg`,
                    "--particle-alpha": `${(0.22 + seeded(i, 17) * 0.58).toFixed(2)}`,
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
    </Show>
  );
}
