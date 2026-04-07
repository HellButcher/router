import { navigatorDetector } from "typesafe-i18n/detectors";
import type { TranslationFunctions } from "./i18n-types.js";
import { loadLocaleAsync } from "./i18n-util.async.js";
import { detectLocale, i18nObject } from "./i18n-util.js";

// Resolve locale from browser preferences, falling back to base locale "en".
// navigator.languages (e.g. ["en-US", "en", "fr"]) is tried in order, so
// region-specific variants naturally fall back to the language-only locale.
function resolveLocale() {
  return detectLocale(navigatorDetector);
}

let _LL: TranslationFunctions | undefined;

export async function initLocale(): Promise<void> {
  const locale = resolveLocale();
  await loadLocaleAsync(locale);
  _LL = i18nObject(locale);
}

export function LL(): TranslationFunctions {
  if (!_LL) throw new Error("initLocale() must be awaited before using LL");
  return _LL;
}
