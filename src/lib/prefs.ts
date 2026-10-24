/**
 * Frontend-local UI preferences (ui-design 00 §8).
 *
 * Persisted in localStorage under `rmccc.ui.*`; regenerable view state that
 * deliberately stays OUT of `UserSettings` to avoid backend schema changes.
 */

const PREFIX = "rmccc.ui.";

export function readPref<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function writePref<T>(key: string, value: T): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Storage may be unavailable (quota / private mode); prefs are best-effort.
  }
}
