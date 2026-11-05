/**
 * Day/night resolution (docs/ui-design/15_THEME_SYSTEM_V4.md §5).
 *
 * Three modes: `off` (theme default), `auto` (follow the OS color scheme) and
 * `schedule` (user-defined start/end times, wrapping across midnight).
 */

export type DayNightMode = "off" | "auto" | "schedule";
export type DayPhase = "day" | "night";

export interface DayNightState {
  mode: DayNightMode;
  /** HH:MM — the moment the day palette takes over. */
  dayStart: string;
  /** HH:MM — the moment the night palette takes over. */
  dayEnd: string;
  /** Manual override used by the quick day/night toggle (`off` mode only). */
  manual: DayPhase | null;
}

export const DEFAULT_DAY_NIGHT: DayNightState = {
  mode: "auto",
  dayStart: "07:00",
  dayEnd: "19:00",
  manual: null,
};

function minutesOf(value: string, fallback: number): number {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) return fallback;
  const hours = Math.min(23, Number(match[1]));
  const minutes = Math.min(59, Number(match[2]));
  return hours * 60 + minutes;
}

export function resolvePhase(
  state: DayNightState,
  fallback: DayPhase,
  systemDark: boolean,
  now = new Date()
): DayPhase {
  if (state.mode === "off") return state.manual ?? fallback;
  if (state.mode === "auto") return systemDark ? "night" : "day";
  const current = now.getHours() * 60 + now.getMinutes();
  const start = minutesOf(state.dayStart, 7 * 60);
  const end = minutesOf(state.dayEnd, 19 * 60);
  const inDay = start <= end
    ? current >= start && current < end
    : current >= start || current < end;
  return inDay ? "day" : "night";
}

export function systemDarkMode(): boolean {
  return typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;
}
