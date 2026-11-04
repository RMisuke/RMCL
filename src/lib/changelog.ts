/**
 * Update log shown by the About page (最近 10 次更新).
 *
 * Data comes from the Vite `virtual:changelog` module, which reads the last
 * ten git commits at dev/build time. Nothing is committed or hand-maintained.
 */
import entries from "virtual:changelog";

export interface ChangelogEntry {
  /** Short commit hash. */
  hash: string;
  /** Commit date (`YYYY-MM-DD`). */
  date: string;
  /** Conventional-commit type (`feat`, `fix`, `docs`, …), when present. */
  type?: string;
  /** Conventional-commit scope, when present. */
  scope?: string;
  title: string;
  notes: string[];
}

export const CHANGELOG: ChangelogEntry[] = entries;

/** Newest-first slice for the About dialog. */
export function recentChangelog(limit = 10): ChangelogEntry[] {
  return CHANGELOG.slice(0, limit);
}
