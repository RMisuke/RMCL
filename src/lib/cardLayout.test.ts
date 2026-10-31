import { describe, expect, it } from "vitest";
import {
  CARD_HEIGHTS,
  CARD_WIDTHS,
  addCard,
  cycleHeight,
  cycleWidth,
  defaultLayout,
  layoutCards,
  migrateLayoutV2,
  normalizePlacement,
  parseLayoutV3,
  parsePluginCardKey,
  pluginCardKey,
  snapHeight,
  snapWidth,
} from "./cardLayout";

describe("card size tiers", () => {
  it("snaps widths to 3/4/6/8/12 and heights to 1..4", () => {
    const loose = { minW: 3, maxW: 12, minH: 1, maxH: 4 };
    expect(CARD_WIDTHS).toEqual([3, 4, 6, 8, 12]);
    expect(CARD_HEIGHTS).toEqual([1, 2, 3, 4]);
    expect(snapWidth(5, loose)).toBe(6);
    expect(snapWidth(5.5, loose)).toBe(6);
    expect(snapWidth(2, loose)).toBe(3);
    expect(snapWidth(24, loose)).toBe(12);
    expect(snapHeight(5, loose)).toBe(4);
    expect(snapHeight(0, loose)).toBe(1);
  });

  it("respects plugin min/max constraints", () => {
    const constrained = { minW: 4, maxW: 8, minH: 2, maxH: 3 };
    expect(snapWidth(12, constrained)).toBe(8);
    expect(snapWidth(3, constrained)).toBe(4);
    expect(snapHeight(1, constrained)).toBe(2);
    expect(snapHeight(4, constrained)).toBe(3);
    expect(cycleWidth(8, 1, constrained)).toBe(4);
    expect(cycleWidth(8, -1, constrained)).toBe(6);
    expect(cycleHeight(3, 1, constrained)).toBe(2);
  });

  it("normalizes arbitrary persisted sizes", () => {
    expect(normalizePlacement("server", 99, 99)).toEqual({ id: "server", w: 12, h: 4 });
    expect(normalizePlacement("compare", 12, 1)).toEqual({ id: "compare", w: 6, h: 2 });
  });
});

describe("layout migration", () => {
  it("starts with server/downloads/versions visible", () => {
    const layout = defaultLayout();
    expect(layoutCards(layout).map((card) => card.id)).toEqual([
      "server",
      "downloads",
      "versions",
    ]);
    expect(layout.hidden).toContain("instances");
    // Hidden cards keep their placement so restore never loses a size.
    expect(layout.cards.some((card) => card.id === "instances")).toBe(true);
  });

  it("migrates v2 order/hidden and preserves legacy plugin ids", () => {
    const layout = migrateLayoutV2({
      order: ["versions", "server", "plugin:dev.example.demo"],
      hidden: ["downloads"],
    });
    expect(layout).not.toBeNull();
    expect(layout!.cards.slice(0, 3).map((card) => card.id)).toEqual([
      "versions",
      "server",
      "plugin:dev.example.demo",
    ]);
    expect(layout!.cards.some((card) => card.id === "downloads")).toBe(true);
    expect(layout!.hidden).toEqual(["downloads"]);
    expect(layout!.cards[0]).toMatchObject({ w: 12, h: 1 });
  });

  it("drops malformed v3 entries and keeps valid ones", () => {
    const layout = parseLayoutV3({
      schemaVersion: 3,
      cards: [
        { id: "server", w: 6, h: 2 },
        { id: "server", w: 12, h: 4 },
        { id: "bad", w: "6", h: 2 },
      ],
      hidden: ["compare", 4],
    });
    expect(layout!.cards[0]).toEqual({ id: "server", w: 6, h: 2 });
    expect(layout!.cards).toHaveLength(7);
    expect(layout!.hidden).toEqual(["compare"]);
  });
});

describe("plugin card keys", () => {
  it("round-trips a plugin card id", () => {
    const key = pluginCardKey("dev.example.demo", "status");
    expect(key).toBe("plugin:dev.example.demo/status");
    expect(parsePluginCardKey(key)).toEqual({ pluginId: "dev.example.demo", cardId: "status" });
    expect(parsePluginCardKey("plugin:dev.example.demo")).toBeNull();
    expect(parsePluginCardKey("server")).toBeNull();
  });

  it("adds and restores a hidden card without duplicates", () => {
    const layout = defaultLayout();
    const withPlugin = addCard(layout, { id: pluginCardKey("dev.example.demo", "a"), w: 4, h: 2 });
    expect(withPlugin.cards).toHaveLength(8);
    expect(addCard(withPlugin, { id: pluginCardKey("dev.example.demo", "a"), w: 6, h: 3 })).toStrictEqual(
      withPlugin
    );
  });
});
