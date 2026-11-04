/**
 * Theme V4 store (docs/ui-design/15_THEME_SYSTEM_V4.md §2).
 *
 * Single owner of the palette state: theme id, day/night mode and the custom
 * base colors. Everything is persisted in one localStorage block; the backend
 * preference is only consulted once for migration.
 */
import { createRoot, createSignal } from "solid-js";
import { applyThemeTokens, setScenePreference } from "../index";
import { adaptiveEnabled, disableAdaptive, enableAdaptive } from "../../lib/adaptiveTheme";
import { settingsStore } from "../../stores/settings";
import { deriveTokens, type Base3 } from "./palette";
import {
  DEFAULT_DAY_NIGHT,
  resolvePhase,
  systemDarkMode,
  type DayNightState,
  type DayPhase,
} from "./daynight";
import { migrateLegacyPreference, themeById, type MotionId } from "./registry";

const STORAGE_KEY = "rmccc.theme.v4";

export interface ThemeV4State {
  themeId: string;
  dayNight: DayNightState;
  custom: Base3;
}

const DEFAULT_CUSTOM: Base3 = {
  accent: "#efb35e",
  background: "#13151a",
  text: "#f2eee7",
};

function readState(): ThemeV4State {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<ThemeV4State>;
      return {
        themeId: themeById(parsed.themeId ?? "") ? (parsed.themeId as string) : migrateLegacyPreference(null),
        dayNight: { ...DEFAULT_DAY_NIGHT, ...(parsed.dayNight ?? {}) },
        custom: { ...DEFAULT_CUSTOM, ...(parsed.custom ?? {}) },
      };
    }
  } catch {
    /* corrupt storage falls through to migration */
  }
  return {
    themeId: migrateLegacyPreference(settingsStore.themePreference()),
    dayNight: { ...DEFAULT_DAY_NIGHT },
    custom: { ...DEFAULT_CUSTOM },
  };
}

function writeState(state: ThemeV4State): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* persistence is best-effort; the live theme still applies */
  }
}

export const themeV4 = createRoot(() => {
  const initial = readState();
  const [themeId, setThemeIdSignal] = createSignal(initial.themeId);
  const [dayNight, setDayNightSignal] = createSignal<DayNightState>(initial.dayNight);
  const [custom, setCustomSignal] = createSignal<Base3>(initial.custom);
  const [phase, setPhase] = createSignal<DayPhase>(
    resolvePhase(initial.dayNight, themeById(initial.themeId)?.defaultMode ?? "night", systemDarkMode())
  );

  let started = false;

  function persist(): void {
    writeState({ themeId: themeId(), dayNight: dayNight(), custom: custom() });
  }

  function motionId(): MotionId | null {
    return themeById(themeId())?.motion ?? null;
  }

  function apply(): void {
    const def = themeById(themeId());
    if (!def) return;
    const root = document.documentElement;
    root.dataset.themeV4 = def.id;

    if (def.kind === "adaptive") {
      setScenePreference(def.id);
      if (!adaptiveEnabled()) void enableAdaptive();
      root.dataset.daynight = phase();
      return;
    }
    if (adaptiveEnabled()) void disableAdaptive(false);

    const nextPhase = resolvePhase(dayNight(), def.defaultMode, systemDarkMode());
    setPhase(nextPhase);
    root.dataset.daynight = nextPhase;

    const base = def.kind === "custom" ? custom() : def[nextPhase];
    setScenePreference(def.id);
    applyThemeTokens(deriveTokens(base));
  }

  function start(): void {
    if (started || typeof window === "undefined") return;
    started = true;
    // Minute-granularity schedule checks; cheap and avoids drift.
    window.setInterval(() => {
      const def = themeById(themeId());
      if (!def || def.kind !== "preset") return;
      const next = resolvePhase(dayNight(), def.defaultMode, systemDarkMode());
      if (next !== phase()) apply();
    }, 30_000);
    window.matchMedia?.("(prefers-color-scheme: dark)")?.addEventListener("change", () => {
      if (dayNight().mode === "auto") apply();
    });
  }

  function setThemeId(id: string): void {
    if (!themeById(id)) return;
    setThemeIdSignal(id);
    persist();
    apply();
  }

  function setDayNight(patch: Partial<DayNightState>): void {
    setDayNightSignal({ ...dayNight(), ...patch });
    persist();
    apply();
  }

  function setCustom(patch: Partial<Base3>): void {
    setCustomSignal({ ...custom(), ...patch });
    persist();
    apply();
  }

  /** Quick manual switch used by the appearance page button. */
  function togglePhase(): void {
    const next: DayPhase = phase() === "day" ? "night" : "day";
    setDayNightSignal({ ...dayNight(), mode: "off", manual: next });
    persist();
    apply();
  }

  return {
    themeId,
    dayNight,
    custom,
    phase,
    motionId,
    setThemeId,
    setDayNight,
    setCustom,
    togglePhase,
    apply,
    start,
  };
});

/** Boot hook: apply the persisted palette and arm the day/night scheduler. */
export function initThemeV4(): void {
  themeV4.apply();
  themeV4.start();
}
