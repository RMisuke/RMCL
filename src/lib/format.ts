/**
 * Display formatting helpers (ui-design 01 §7: relative times, size/count
 * abbreviations, durations). Pure functions — unit-testable.
 */

/** `172800000` → "2 天前"; null → never. */
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const diff = Date.now() - then;
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "<1 min";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} mo`;
  return `${Math.floor(months / 12)} y`;
}

/** ISO timestamp → local `YYYY-MM-DD` (version release dates); "" when invalid. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** `1893` → "1.9K", `1234567` → "1.2M". */
export function abbreviateCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** `1536` → "1.5 KiB"; binary units, matching the launcher's byte accounting. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${Math.round(value)} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
}

/** Milliseconds → "12 min" / "1 h 05 min" (running-game timer). */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.max(0, Math.floor(ms / 60_000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes} min`;
  return `${hours} h ${String(minutes).padStart(2, "0")} min`;
}

/** `12345678` → "12,345,678" (download file counters). */
export function groupDigits(n: number): string {
  return n.toLocaleString("en-US");
}

/** First letter for icon placeholders (mod icons, profiles). */
export function initialOf(name: string): string {
  const trimmed = name.trim();
  return trimmed ? trimmed[0].toUpperCase() : "?";
}

/** Long paths: keep both ends visible (`/Volumes/…/mods`). */
export function elidePath(path: string, max = 52): string {
  if (path.length <= max) return path;
  const head = Math.floor((max - 1) * 0.45);
  return `${path.slice(0, head)}…${path.slice(-(max - head - 1))}`;
}
