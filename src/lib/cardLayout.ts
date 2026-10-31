/**
 * Home dashboard card layout v3.
 *
 * Pure functions only: persistence lives in Home.tsx, so every sizing/
 * migration rule is unit-testable without a DOM. The canonical tiers are
 * shared by CSS, the manifest schema and WidgetDescriptor consumers.
 */

export const CARD_WIDTHS = [3, 4, 6, 8, 12] as const;
export const CARD_HEIGHTS = [1, 2, 3, 4] as const;

export type CardWidth = (typeof CARD_WIDTHS)[number];
export type CardHeight = (typeof CARD_HEIGHTS)[number];

export interface CardPlacement {
  id: string;
  w: CardWidth;
  h: CardHeight;
}

export interface HomeLayoutV3 {
  schemaVersion: 3;
  cards: CardPlacement[];
  hidden: string[];
}

export interface CardConstraints {
  minW: number;
  maxW: number;
  minH: number;
  maxH: number;
}

export const BUILTIN_CARD_IDS = [
  "server",
  "versions",
  "downloads",
  "instances",
  "announcements",
  "compare",
  "games",
] as const;

export type BuiltinCardId = (typeof BUILTIN_CARD_IDS)[number];

export const DEFAULT_CARD_SIZES: Record<BuiltinCardId, CardPlacement> = {
  server: { id: "server", w: 6, h: 2 },
  versions: { id: "versions", w: 12, h: 1 },
  downloads: { id: "downloads", w: 6, h: 2 },
  instances: { id: "instances", w: 6, h: 2 },
  announcements: { id: "announcements", w: 6, h: 3 },
  compare: { id: "compare", w: 4, h: 2 },
  games: { id: "games", w: 6, h: 2 },
};

export const DEFAULT_CARD_CONSTRAINTS: CardConstraints = {
  minW: 3,
  maxW: 12,
  minH: 1,
  maxH: 4,
};

export const BUILTIN_CARD_CONSTRAINTS: Record<BuiltinCardId, CardConstraints> = {
  server: { minW: 3, maxW: 12, minH: 2, maxH: 4 },
  versions: { minW: 4, maxW: 12, minH: 1, maxH: 3 },
  downloads: { minW: 3, maxW: 12, minH: 2, maxH: 4 },
  instances: { minW: 3, maxW: 8, minH: 2, maxH: 4 },
  announcements: { minW: 4, maxW: 12, minH: 2, maxH: 4 },
  compare: { minW: 3, maxW: 6, minH: 2, maxH: 3 },
  games: { minW: 4, maxW: 12, minH: 2, maxH: 3 },
};

export const DEFAULT_VISIBLE_CARDS = ["server", "downloads", "versions"] as const;
export const DEFAULT_HIDDEN_CARDS = ["instances", "announcements", "compare", "games"] as const;

export const LAYOUT_KEY_V3 = "rmccc.softDashboard.v3";
export const LAYOUT_KEY_V2 = "rmccc.softDashboard.v2";

export function isBuiltinCardId(id: string): id is BuiltinCardId {
  return (BUILTIN_CARD_IDS as readonly string[]).includes(id);
}

export function constraintsForBuiltin(id: BuiltinCardId): CardConstraints {
  return BUILTIN_CARD_CONSTRAINTS[id];
}

export function defaultLayout(): HomeLayoutV3 {
  return {
    schemaVersion: 3,
    cards: [...DEFAULT_VISIBLE_CARDS, ...DEFAULT_HIDDEN_CARDS].map((id) => ({
      ...DEFAULT_CARD_SIZES[id],
    })),
    hidden: [...DEFAULT_HIDDEN_CARDS],
  };
}

/** Nearest allowed tier; ties choose the larger tier for stable growth. */
function nearestTier(value: number, tiers: readonly number[], min: number, max: number): number {
  const allowed = tiers.filter((tier) => tier >= min && tier <= max);
  if (allowed.length === 0) {
    const fallback = tiers.find((tier) => tier >= Math.min(min, max)) ?? tiers[tiers.length - 1];
    return fallback;
  }
  let best = allowed[0];
  let bestDistance = Math.abs(value - best);
  for (const tier of allowed.slice(1)) {
    const distance = Math.abs(value - tier);
    if (distance < bestDistance || (distance === bestDistance && tier > best)) {
      best = tier;
      bestDistance = distance;
    }
  }
  return best;
}

export function snapWidth(value: number, constraints: CardConstraints): CardWidth {
  return nearestTier(value, CARD_WIDTHS, constraints.minW, constraints.maxW) as CardWidth;
}

export function snapHeight(value: number, constraints: CardConstraints): CardHeight {
  return nearestTier(value, CARD_HEIGHTS, constraints.minH, constraints.maxH) as CardHeight;
}

export function normalizePlacement(
  id: string,
  width: number,
  height: number,
  constraints?: CardConstraints
): CardPlacement {
  const effective =
    constraints ??
    (isBuiltinCardId(id) ? constraintsForBuiltin(id) : DEFAULT_CARD_CONSTRAINTS);
  return {
    id,
    w: snapWidth(width, effective),
    h: snapHeight(height, effective),
  };
}

/** Cycle through only the tiers allowed by this card's min/max. */
export function cycleWidth(
  current: CardWidth,
  delta: 1 | -1,
  constraints: CardConstraints = DEFAULT_CARD_CONSTRAINTS
): CardWidth {
  const allowed = CARD_WIDTHS.filter((w) => w >= constraints.minW && w <= constraints.maxW);
  if (allowed.length === 0) return current;
  const index = allowed.indexOf(current);
  const next = index < 0 ? 0 : (index + delta + allowed.length) % allowed.length;
  return allowed[next];
}

export function cycleHeight(
  current: CardHeight,
  delta: 1 | -1,
  constraints: CardConstraints = DEFAULT_CARD_CONSTRAINTS
): CardHeight {
  const allowed = CARD_HEIGHTS.filter((h) => h >= constraints.minH && h <= constraints.maxH);
  if (allowed.length === 0) return current;
  const index = allowed.indexOf(current);
  const next = index < 0 ? 0 : (index + delta + allowed.length) % allowed.length;
  return allowed[next];
}

export function pluginCardKey(pluginId: string, cardId: string): string {
  return `plugin:${pluginId}/${cardId}`;
}

export function parsePluginCardKey(
  id: string
): { pluginId: string; cardId: string } | null {
  if (!id.startsWith("plugin:")) return null;
  const rest = id.slice("plugin:".length);
  const slash = rest.indexOf("/");
  if (slash <= 0 || slash === rest.length - 1) return null;
  return { pluginId: rest.slice(0, slash), cardId: rest.slice(slash + 1) };
}

/** Legacy v2 key `plugin:<pluginId>` (one card per plugin). */
export function parseLegacyPluginKey(id: string): string | null {
  if (!id.startsWith("plugin:")) return null;
  const pluginId = id.slice("plugin:".length);
  return pluginId && !pluginId.includes("/") ? pluginId : null;
}

function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

/**
 * Parse a persisted v3 layout. Unknown/invalid entries are dropped; known
 * builtin cards missing from an otherwise-valid layout are appended so a
 * partial storage write cannot make a default card disappear forever.
 */
export function parseLayoutV3(value: unknown): HomeLayoutV3 | null {
  if (typeof value !== "object" || value === null) return null;
  const raw = value as Record<string, unknown>;
  if (raw.schemaVersion !== 3 || !Array.isArray(raw.cards)) return null;
  const seen = new Set<string>();
  const cards: CardPlacement[] = [];
  for (const item of raw.cards) {
    if (typeof item !== "object" || item === null) continue;
    const entry = item as Record<string, unknown>;
    if (typeof entry.id !== "string" || entry.id.length === 0 || seen.has(entry.id)) continue;
    const w = asNumber(entry.w);
    const h = asNumber(entry.h);
    if (w === null || h === null) continue;
    const constraints = isBuiltinCardId(entry.id)
      ? constraintsForBuiltin(entry.id)
      : DEFAULT_CARD_CONSTRAINTS;
    cards.push(normalizePlacement(entry.id, w, h, constraints));
    seen.add(entry.id);
  }
  const hidden = [...new Set(asStringArray(raw.hidden))];
  if (cards.length === 0) return null;
  // Older writes may have omitted hidden placements. Keep every builtin
  // restorable by appending the missing defaults in canonical order.
  for (const id of BUILTIN_CARD_IDS) {
    if (!seen.has(id)) cards.push({ ...DEFAULT_CARD_SIZES[id] });
  }
  return { schemaVersion: 3, cards, hidden };
}

/** Migrate the pre-v3 `{ order, hidden }` shape. */
export function migrateLayoutV2(value: unknown): HomeLayoutV3 | null {
  if (typeof value !== "object" || value === null) return null;
  const raw = value as Record<string, unknown>;
  if (!Array.isArray(raw.order)) return null;
  const order = asStringArray(raw.order);
  const hidden = [...new Set(asStringArray(raw.hidden))];
  const cards: CardPlacement[] = [];
  const seen = new Set<string>();
  for (const id of order) {
    if (seen.has(id)) continue;
    if (isBuiltinCardId(id)) {
      const placement = DEFAULT_CARD_SIZES[id];
      cards.push({ ...placement });
      seen.add(id);
    } else if (id.startsWith("plugin:")) {
      // Keep old plugin ids: Home reconciles `plugin:<pluginId>` to the
      // plugin's first contributed card once the catalog is available.
      cards.push({ id, w: 4, h: 2 });
      seen.add(id);
    }
  }
  for (const id of BUILTIN_CARD_IDS) {
    if (!seen.has(id)) cards.push({ ...DEFAULT_CARD_SIZES[id] });
  }
  if (cards.length === 0) return null;
  return { schemaVersion: 3, cards, hidden };
}

export function layoutCards(layout: HomeLayoutV3): CardPlacement[] {
  return layout.cards.filter((card) => !layout.hidden.includes(card.id));
}

export function setCardSize(
  layout: HomeLayoutV3,
  id: string,
  dimensions: { w?: CardWidth; h?: CardHeight }
): HomeLayoutV3 {
  return {
    ...layout,
    cards: layout.cards.map((card) =>
      card.id === id
        ? { ...card, w: dimensions.w ?? card.w, h: dimensions.h ?? card.h }
        : card
    ),
  };
}

export function reorderCard(layout: HomeLayoutV3, id: string, delta: -1 | 1): HomeLayoutV3 {
  const cards = [...layout.cards];
  const index = cards.findIndex((card) => card.id === id);
  const target = index + delta;
  if (index < 0 || target < 0 || target >= cards.length) return layout;
  [cards[index], cards[target]] = [cards[target], cards[index]];
  return { ...layout, cards };
}

export function hideCard(layout: HomeLayoutV3, id: string): HomeLayoutV3 {
  if (layout.hidden.includes(id)) return layout;
  return { ...layout, hidden: [...layout.hidden, id] };
}

export function restoreCard(layout: HomeLayoutV3, id: string): HomeLayoutV3 {
  if (!layout.hidden.includes(id)) return layout;
  return { ...layout, hidden: layout.hidden.filter((item) => item !== id) };
}

export function addCard(layout: HomeLayoutV3, placement: CardPlacement): HomeLayoutV3 {
  const exists = layout.cards.some((card) => card.id === placement.id);
  return {
    ...layout,
    cards: exists
      ? layout.cards
      : [...layout.cards, { ...placement, w: placement.w, h: placement.h }],
    hidden: layout.hidden.filter((id) => id !== placement.id),
  };
}
