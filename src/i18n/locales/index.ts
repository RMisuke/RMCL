/**
 * Locale registry — the single place that knows which languages the launcher
 * ships on the frontend.
 *
 * NOTE: keep `BUNDLES` aligned with `rmccc_config::SUPPORTED_LOCALES` on the
 * Rust side (it owns validation and the picker's display names). If the backend
 * ever offers a locale with no bundle here, `i18n/index.ts` falls back to
 * English for that locale rather than rendering raw keys.
 */
import { en } from "./en";
import { zhCN } from "./zh-CN";
import type { Messages } from "./messages";

/** Locale used when nothing else matches. Mirrors `rmccc_config::DEFAULT_LOCALE`. */
export const DEFAULT_LOCALE = "en";

/** Translation bundles keyed by BCP-47 tag. */
export const BUNDLES: Record<string, Messages> = {
  en,
  "zh-CN": zhCN,
};

/** Locale codes this frontend can actually translate. */
export const AVAILABLE_LOCALES = Object.keys(BUNDLES);

/**
 * Normalises an arbitrary BCP-47 tag (or bare language code) to a locale we
 * ship, falling back to {@link DEFAULT_LOCALE}.
 *
 * Mirrors `rmccc_config::resolve_locale` so the persisted value and the
 * rendered language agree: `en-US` → `en`, `zh-Hans-CN` → `zh-CN`, `fr` → `en`.
 */
export function normalizeLocale(requested: string | null | undefined): string {
  const tag = requested?.trim();
  if (!tag) return DEFAULT_LOCALE;

  const lower = tag.toLowerCase().replace(/_/g, "-");

  const exact = AVAILABLE_LOCALES.find((c) => c.toLowerCase() === lower);
  if (exact) return exact;

  const primary = lower.split("-")[0];
  const byLanguage = AVAILABLE_LOCALES.find((c) => c.split("-")[0] === primary);
  return byLanguage ?? DEFAULT_LOCALE;
}
