/** Debug switch: show the first-run wizard on the next launch. */
const FORCE_KEY = "rmccc.onboarding.forceNext";

export function hasOnboardingForce(): boolean {
  try {
    return localStorage.getItem(FORCE_KEY) === "1";
  } catch {
    return false;
  }
}

export function requestOnboardingOnNextLaunch(): void {
  try {
    localStorage.setItem(FORCE_KEY, "1");
  } catch {
    /* localStorage unavailable — the debug switch is best-effort */
  }
}

export function clearOnboardingForce(): void {
  try {
    localStorage.removeItem(FORCE_KEY);
  } catch {
    /* ignore */
  }
}
