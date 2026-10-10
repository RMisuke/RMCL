/**
 * Types derived from the reference (English) bundle.
 *
 * Kept separate so locale bundles can be typed without importing each other
 * (which would create a cycle between `en`, `zh-CN` and the registry).
 */
import type { en } from "./en";

/** Every valid translation key, derived from the English bundle. */
export type TranslationKey = keyof typeof en;

/**
 * Shape every locale bundle must satisfy: exactly the English keys, no more
 * and no fewer (object-literal assignment is excess-property checked).
 */
export type Messages = Record<TranslationKey, string>;
