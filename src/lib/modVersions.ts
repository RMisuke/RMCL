/**
 * Mod-version grouping helpers (HMCL-style "Minecraft 1.21.11" sections).
 *
 * Modrinth lists every compatible game version on each file, so naïvely
 * rendering one row per game version produces hundreds of near-duplicates.
 * We group by one primary game version, keep the full compatibility list on
 * the row, and re-group around an explicit filter when the user picks one.
 */
import type { ModVersion } from "../ipc/types";
import { normalizeLoaderId } from "./loaders";

export type VersionChannel = "release" | "beta" | "alpha";

const SNAPSHOT_RE = /^(?:\d{2}w\d{2}[a-z]|\d+\.\d+(?:\.\d+)?-(?:pre|rc)-?\d+)$/i;

/** True for weekly snapshots (`25w45a`) and pre-release/RC builds. */
export function isSnapshotGameVersion(version: string): boolean {
  return SNAPSHOT_RE.test(version.trim());
}

/** `1.21.5-pre2` → `1.21.5`; weekly snapshots are returned unchanged. */
export function baseGameVersion(version: string): string {
  return version.trim().replace(/-(?:pre|rc)-?\d+$/i, "");
}

interface ParsedVersion {
  kind: 0 | 1 | 2; // 0 = numbered release, 1 = weekly snapshot, 2 = unknown
  numbers: number[];
  suffix: number; // 0 = final, 1 = rc, 2 = pre
  raw: string;
}

function parseGameVersion(version: string): ParsedVersion {
  const raw = version.trim().toLowerCase();
  const weekly = /^(\d{2})w(\d{2})([a-z])$/.exec(raw);
  if (weekly) {
    return {
      kind: 1,
      numbers: [Number(weekly[1]), Number(weekly[2]), weekly[3].charCodeAt(0) - 96],
      suffix: 0,
      raw,
    };
  }
  const pre = /-(pre|rc)-?(\d+)$/.exec(raw);
  const suffix = pre ? (pre[1] === "rc" ? 1 : 2) : 0;
  const numbered = /^(\d+)(?:\.(\d+))?(?:\.(\d+))?/.exec(raw);
  if (numbered) {
    return {
      kind: 0,
      numbers: [Number(numbered[1]), Number(numbered[2] ?? 0), Number(numbered[3] ?? 0)],
      suffix,
      raw,
    };
  }
  return { kind: 2, numbers: [], suffix: 0, raw };
}

/** Ascending game-version order: `1.20.4 < 1.21-pre1 < 1.21 < 1.21.11`. */
export function compareGameVersions(a: string, b: string): number {
  const pa = parseGameVersion(a);
  const pb = parseGameVersion(b);
  // Numbered releases sort before weekly snapshots so release groups stay on
  // top; within numbered releases prereleases sort just below their release.
  if (pa.kind !== pb.kind) {
    if (pa.kind === 0) return 1;
    if (pb.kind === 0) return -1;
    return 0;
  }
  const length = Math.max(pa.numbers.length, pb.numbers.length);
  for (let i = 0; i < length; i += 1) {
    const diff = (pa.numbers[i] ?? 0) - (pb.numbers[i] ?? 0);
    if (diff !== 0) return diff;
  }
  // Same base: final > rc > pre.
  if (pa.suffix !== pb.suffix) return pa.suffix === 0 ? 1 : pb.suffix === 0 ? -1 : pb.suffix - pa.suffix;
  return pa.raw.localeCompare(pb.raw);
}

/** Unique game versions, newest first. */
export function sortGameVersionsDesc(versions: Iterable<string>): string[] {
  return [...new Set(versions)].sort((a, b) => compareGameVersions(b, a));
}

/** Normalized release channel; unknown upstream values fall back to release. */
export function versionChannel(version: ModVersion): VersionChannel {
  const value = (version.version_type ?? "release").toLowerCase();
  if (value === "alpha" || value === "beta") return value;
  return "release";
}

/** Channel ordering for the filter chips: stable first. */
export function channelRank(channel: VersionChannel): number {
  return channel === "release" ? 0 : channel === "beta" ? 1 : 2;
}

function publishedTime(version: ModVersion): number {
  const parsed = version.published ? Date.parse(version.published) : Number.NaN;
  return Number.isNaN(parsed) ? 0 : parsed;
}

function compareVersionRecency(a: ModVersion, b: ModVersion): number {
  const published = publishedTime(b) - publishedTime(a);
  if (published !== 0) return published;
  return b.version_number.localeCompare(a.version_number, undefined, { numeric: true });
}

/**
 * Choose the version directory a file belongs to. With a `focus` (the user's
 * game-version filter) we honour that version when the file supports it;
 * otherwise use the newest numbered release the file supports.
 */
export function primaryGameVersion(version: ModVersion, focus?: string): string | null {
  const supported = version.game_versions;
  if (focus && supported.includes(focus)) return focus;
  if (supported.length === 0) return null;
  const ordered = sortGameVersionsDesc(supported);
  return ordered.find((v) => !isSnapshotGameVersion(v)) ?? ordered[0];
}

export interface ModVersionGroup {
  /** Stable key: `r:1.21.11` / `s:25w45a` (avoids collision with labels). */
  key: string;
  /** Base game version shown in the group heading (`1.21.11`). */
  gameVersion: string;
  snapshot: boolean;
  versions: ModVersion[];
  latest: ModVersion;
}

/** Group files under `Minecraft <version>` headings, newest group first. */
export function groupModVersions(versions: ModVersion[], focus?: string): ModVersionGroup[] {
  const groups = new Map<string, ModVersionGroup>();
  for (const version of versions) {
    const gameVersion = primaryGameVersion(version, focus);
    const label = gameVersion ?? "";
    const snapshot = gameVersion ? isSnapshotGameVersion(gameVersion) : false;
    const key = `${snapshot ? "s" : "r"}:${label}`;
    const existing = groups.get(key);
    if (existing) {
      existing.versions.push(version);
    } else {
      groups.set(key, {
        key,
        gameVersion: label,
        snapshot,
        versions: [version],
        latest: version,
      });
    }
  }
  for (const group of groups.values()) {
    group.versions.sort(compareVersionRecency);
    group.latest = group.versions[0];
  }
  return [...groups.values()].sort((a, b) => {
    if (a.gameVersion === "" && b.gameVersion !== "") return 1;
    if (b.gameVersion === "" && a.gameVersion !== "") return -1;
    // Snapshot groups after release groups when the version string is equal.
    if (a.snapshot !== b.snapshot) return a.snapshot ? 1 : -1;
    return compareGameVersions(b.gameVersion, a.gameVersion);
  });
}

/** Deduplicate/filter helper used by the detail toolbar. */
export function filterModVersions(
  versions: ModVersion[],
  options: { gameVersion?: string; loader?: string; channel?: VersionChannel | "all" },
): ModVersion[] {
  return versions.filter((version) => {
    if (options.gameVersion && !version.game_versions.includes(options.gameVersion)) return false;
    if (
      options.loader &&
      !version.loaders.some((l) => normalizeLoaderId(l) === normalizeLoaderId(options.loader))
    ) {
      return false;
    }
    if (options.channel && options.channel !== "all" && versionChannel(version) !== options.channel) {
      return false;
    }
    return true;
  });
}
