import { describe, expect, it } from "vitest";
import {
  abbreviateCount,
  elidePath,
  formatBytes,
  formatDate,
  formatDuration,
  groupDigits,
  initialOf,
  playtimeParts,
  providerLabel,
  relativeTime,
} from "./format";

describe("relativeTime", () => {
  it("empty/invalid → empty string", () => {
    expect(relativeTime(null)).toBe("");
    expect(relativeTime(undefined)).toBe("");
    expect(relativeTime("not a date")).toBe("");
  });

  it("buckets minutes/hours/days/months/years", () => {
    const now = Date.now();
    expect(relativeTime(new Date(now - 30_000).toISOString())).toBe("<1 min");
    expect(relativeTime(new Date(now - 5 * 60_000).toISOString())).toBe("5 min");
    expect(relativeTime(new Date(now - 3 * 3_600_000).toISOString())).toBe("3 h");
    expect(relativeTime(new Date(now - 2 * 86_400_000).toISOString())).toBe("2 d");
    expect(relativeTime(new Date(now - 45 * 86_400_000).toISOString())).toBe("1 mo");
    expect(relativeTime(new Date(now - 800 * 86_400_000).toISOString())).toBe("2 y");
  });
});

describe("formatDate", () => {
  it("ISO → local YYYY-MM-DD; invalid → empty", () => {
    expect(formatDate("2026-10-26T00:00:00Z")).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(formatDate("garbage")).toBe("");
    expect(formatDate(null)).toBe("");
  });
});

describe("abbreviateCount", () => {
  it("scales K/M", () => {
    expect(abbreviateCount(999)).toBe("999");
    expect(abbreviateCount(1893)).toBe("1.9K");
    expect(abbreviateCount(1_234_567)).toBe("1.2M");
  });
});

describe("formatBytes", () => {
  it("binary units, one decimal past bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KiB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MiB");
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });
});

describe("formatDuration", () => {
  it("minutes and h+mm", () => {
    expect(formatDuration(5 * 60_000)).toBe("5 min");
    expect(formatDuration(65 * 60_000)).toBe("1 h 05 min");
    expect(formatDuration(-10)).toBe("0 min");
  });
});

describe("playtimeParts", () => {
  it("splits hours/minutes and clamps negatives", () => {
    expect(playtimeParts(0)).toEqual({ hours: 0, minutes: 0 });
    expect(playtimeParts(59)).toEqual({ hours: 0, minutes: 0 });
    expect(playtimeParts(3_600)).toEqual({ hours: 1, minutes: 0 });
    expect(playtimeParts(3 * 3600 + 20 * 60 + 5)).toEqual({ hours: 3, minutes: 20 });
    expect(playtimeParts(-5)).toEqual({ hours: 0, minutes: 0 });
  });
});

describe("providerLabel", () => {
  it("keeps brand casing and capitalizes known providers", () => {
    expect(providerLabel("microsoft")).toBe("Microsoft");
    expect(providerLabel("offline")).toBe("Offline");
    expect(providerLabel("custom")).toBe("custom");
  });
});

describe("groupDigits / initialOf / elidePath", () => {
  it("groups thousands", () => {
    expect(groupDigits(12_345_678)).toBe("12,345,678");
  });

  it("initialOf uppercases the first grapheme; blank → ?", () => {
    expect(initialOf("sodium")).toBe("S");
    expect(initialOf("  fabric ")).toBe("F");
    expect(initialOf("")).toBe("?");
  });

  it("elidePath keeps both ends", () => {
    const long = "/Volumes/APPLE 1TB/Code/RMCL/instances/sodium/submods/extra/mods";
    expect(elidePath(long)).toContain("…");
    expect(elidePath(long).startsWith("/Vol")).toBe(true);
    expect(elidePath(long).endsWith("mods")).toBe(true);
    expect(elidePath("short/path")).toBe("short/path");
  });
});
