/**
 * Home "background only" mode.
 *
 * When on, the greeting/search/edit chrome and every home card are hidden so
 * the wallpaper or local video is fully visible. The toggle itself stays in the
 * top-right corner, and the bottom docks remain available for navigation.
 *
 * Persisted locally like the other appearance preferences.
 */
import { createRoot, createSignal } from "solid-js";

const STORAGE_KEY = "rmccc.backgroundOnly.v1";

function read(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

const [backgroundOnly, setBackgroundOnlySignal] = createRoot(() => createSignal(read()));

export function backgroundOnlyMode(): boolean {
  return backgroundOnly();
}

export function setBackgroundOnlyMode(value: boolean): void {
  setBackgroundOnlySignal(value);
  try {
    localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
  } catch {
    // Best-effort persistence; the toggle still works for this session.
  }
}

export function toggleBackgroundOnlyMode(): void {
  setBackgroundOnlyMode(!backgroundOnly());
}
