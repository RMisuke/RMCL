/**
 * i18n runtime.
 *
 * Resolution order for the *effective* locale:
 *   1. `UserSettings.locale` when it pins a language (`"en"`, `"zh-CN"`);
 *   2. otherwise the operating-system language as reported by the WebView
 *      (`navigator.languages`);
 *   3. falling back to `DEFAULT_LOCALE` (English) when the system language is
 *      not one we ship — `fr`, `ja`, … render English rather than raw keys.
 *
 * Steps 2 and 3 go through `normalizeLocale`, which mirrors the Rust side
 * (`rmccc_config::resolve_locale`) so what we render matches what the backend
 * persisted.
 *
 * `t()` reads a Solid signal, so components calling it inside JSX re-render
 * automatically when the language changes.
 */
import { createRoot, createSignal } from "solid-js";
import { AVAILABLE_LOCALES, BUNDLES, DEFAULT_LOCALE, normalizeLocale } from "./locales";
import type { TranslationKey } from "./locales/messages";
import { userSettings, type LocaleInfo } from "../ipc/api";

/** `null` = follow the system language. */
export type LocalePreference = string | null;

const { locale, preference, supportedLocales, setLocale_, setPreference_, setSupported_ } =
  createRoot(() => {
    const [locale, setLocale_] = createSignal(DEFAULT_LOCALE);
    const [preference, setPreference_] = createSignal<LocalePreference>(null);
    const [supportedLocales, setSupported_] = createSignal<LocaleInfo[]>([]);
    return {
      locale,
      preference,
      supportedLocales,
      setLocale_,
      setPreference_,
      setSupported_,
    };
  });

/** Effective locale currently rendered (reactive). */
export { locale };

/** Persisted preference: `null` means "follow the system language" (reactive). */
export { preference };

/** Locales the backend reports as supported, with their native names. */
export { supportedLocales };

/** Languages reported by the OS, most preferred first. */
function systemLocaleTags(): string[] {
  const list = navigator.languages;
  if (list && list.length > 0) return [...list];
  return navigator.language ? [navigator.language] : [];
}

/**
 * The system language, normalised to a locale we ship, or English when the OS
 * language is unsupported.
 *
 * Matching is on the primary subtag, so `en-GB` → `en` and any Chinese variant
 * (`zh`, `zh-TW`, `zh-Hans`) → `zh-CN` — the only Chinese locale we ship.
 */
export function systemLocale(): string {
  for (const tag of systemLocaleTags()) {
    const primary = tag.toLowerCase().replace(/_/g, "-").split("-")[0];
    const match = AVAILABLE_LOCALES.find((code) => code.split("-")[0] === primary);
    if (match) return match;
  }
  return DEFAULT_LOCALE;
}

function apply(next: string): void {
  setLocale_(next);
  // Keep the document language in sync (affects font and line-breaking rules).
  document.documentElement.lang = next;
}

/**
 * Translates `key`, substituting `{name}` placeholders.
 *
 * Falls back to English and finally to the key itself, so a gap in a bundle can
 * never render as an empty string.
 */
export function t(
  key: TranslationKey,
  params?: Record<string, string | number>
): string {
  const bundle = BUNDLES[locale()] ?? BUNDLES[DEFAULT_LOCALE];
  let text: string = bundle[key] ?? BUNDLES[DEFAULT_LOCALE][key] ?? key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

/** Native name for a locale code, taken from the backend's list. */
export function nativeNameFor(code: string): string {
  return supportedLocales().find((l) => l.code === code)?.native_name ?? code;
}

/**
 * Loads the persisted preference and applies the effective locale.
 *
 * Call once before revealing the window so the first paint is already in the
 * right language. Never throws: an unreachable backend degrades to following
 * the system language.
 */
export async function initI18n(): Promise<void> {
  // The persisted preference and the supported-locale list are independent;
  // fetch them concurrently (perf audit 3.1: this used to be two serial IPC
  // round-trips gating the first paint).
  const [stored, supported] = await Promise.all([
    userSettings.get(),
    userSettings.supportedLocales(),
  ]);
  let pref: LocalePreference = null;
  if (stored.ok) {
    pref = stored.data.locale && stored.data.locale.trim() !== "" ? stored.data.locale : null;
  } else {
    console.warn("i18n: could not read user settings, following system language", stored.error);
  }
  setPreference_(pref);
  apply(pref ? normalizeLocale(pref) : systemLocale());

  if (supported.ok) {
    setSupported_(supported.data);
  } else {
    console.warn("i18n: could not load the supported locale list", supported.error);
  }
}

/**
 * Switches the language and persists the choice.
 *
 * `null` stores "follow the system language" (the backend clears the override).
 */
export async function setLocale(next: LocalePreference): Promise<void> {
  setPreference_(next);
  apply(next ? normalizeLocale(next) : systemLocale());
  const saved = await userSettings.set({ locale: next ?? "", check_updates: null, theme: null });
  if (!saved.ok) {
    console.warn("i18n: could not persist the locale preference", saved.error);
  }
}

/** Re-resolves the effective locale after an OS language change. */
export function refreshFromSystem(): void {
  if (preference() === null) apply(systemLocale());
}
