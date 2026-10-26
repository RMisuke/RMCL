import { describe, expect, it } from "vitest";
import { isCacheFresh } from "./versionCache";

// Importing `./versionCache` pulls in ipc/api (Tauri invoke) and solid-js;
// both are import-safe under Node — invoke is only called at runtime.

const FRESH_MS = 10 * 60 * 1000;

describe("isCacheFresh", () => {
  it("fresh inside the TTL window", () => {
    expect(isCacheFresh(Date.now(), Date.now())).toBe(true);
    expect(isCacheFresh(Date.now() - FRESH_MS + 1_000, Date.now())).toBe(true);
  });

  it("stale at/after the window", () => {
    expect(isCacheFresh(Date.now() - FRESH_MS, Date.now())).toBe(false);
    expect(isCacheFresh(Date.now() - FRESH_MS - 60_000, Date.now())).toBe(false);
  });
});
