import { describe, expect, it } from "vitest";
import { LOADER_CARDS, loaderLabel } from "./loaders";

describe("loader catalog", () => {
  it("ids are unique and non-empty", () => {
    const ids = LOADER_CARDS.map((l) => l.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id.trim().length).toBeGreaterThan(0);
  });

  it("labels differ from raw ids (user-facing names)", () => {
    for (const card of LOADER_CARDS) {
      expect(loaderLabel(card.id)).toBe(card.label);
      expect(card.label).not.toBe(card.id); // e.g. neo_forge → NeoForge
    }
  });

  it("unknown ids fall back to the raw id", () => {
    expect(loaderLabel("cleanroom")).toBe("cleanroom");
    expect(loaderLabel(null)).toBe("");
    expect(loaderLabel(undefined)).toBe("");
  });
});
