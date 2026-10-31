import { describe, expect, it } from "vitest";
import { LOADER_CARDS, loaderLabel, normalizeLoaderId } from "./loaders";

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

  it("normalizes Modrinth / aliased loader ids", () => {
    expect(normalizeLoaderId("neoforge")).toBe("neo_forge");
    expect(normalizeLoaderId("liteloader")).toBe("lite_loader");
    expect(loaderLabel("neoforge")).toBe("NeoForge");
    expect(loaderLabel("liteloader")).toBe("LiteLoader");
  });

  it("unknown ids fall back to the raw id", () => {
    expect(loaderLabel("cleanroom")).toBe("cleanroom");
    expect(loaderLabel(null)).toBe("");
    expect(loaderLabel(undefined)).toBe("");
  });
});
