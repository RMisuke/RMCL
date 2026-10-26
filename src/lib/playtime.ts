/**
 * Cumulative play-time label shared by the home launch dock and instance cards.
 * The duration is stored in seconds (`Profile.play_time_seconds`).
 */
import { t } from "../i18n";
import { playtimeParts } from "./format";

/** `45240` → "12 小时 34 分" / "12 h 34 min"; sub-minute sessions → "不到 1 分钟". */
export function playtimeLabel(seconds: number): string {
  const { hours, minutes } = playtimeParts(seconds);
  if (hours > 0 && minutes > 0) return t("playtime.hoursMinutes", { h: hours, m: minutes });
  if (hours > 0) return t("playtime.hours", { h: hours });
  if (minutes > 0) return t("playtime.minutes", { m: minutes });
  return t("playtime.lessThanMinute");
}
