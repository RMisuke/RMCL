/**
 * Display labels for the two version streams.
 *
 * SNGL follows semver and carries an `Alpha` stage suffix. RMCCC is shown as a
 * two-part version (`0.1`, `0.2`, …) derived from the core crate's major.minor,
 * matching the bump rule: frontend changes bump SNGL's patch, backend changes
 * bump RMCCC's minor.
 */
export const VERSION_STAGE = "Alpha";

export function launcherVersionLabel(version: string | null | undefined): string {
  return `${version ?? "0.0.0"} ${VERSION_STAGE}`;
}

export function coreVersionLabel(version: string | null | undefined): string {
  const match = /^(\d+)\.(\d+)/.exec(version ?? "");
  return `${match ? `${match[1]}.${match[2]}` : "0.1"} ${VERSION_STAGE}`;
}
