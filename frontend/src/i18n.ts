import type { AppPrefs } from "./types";

type Locale = NonNullable<AppPrefs["locale"]>;
type Dictionary = Record<string, unknown>;

const loaders: Record<Locale, () => Promise<{ default: string }>> = {
  az: () => import("./locales/app_az.arb?raw"),
  en: () => import("./locales/app_en.arb?raw"),
  ru: () => import("./locales/app_ru.arb?raw"),
  tr: () => import("./locales/app_tr.arb?raw"),
};
const dictionaries: Partial<Record<Locale, Dictionary>> = {};

/** Loads a locale catalog; `makeStrings` needs it loaded before first use. */
export async function loadLocale(locale: Locale): Promise<void> {
  dictionaries[locale] ??= JSON.parse(
    (await loaders[locale]()).default,
  ) as Dictionary;
  document.documentElement.lang = locale;
}

export function systemLocale(): Locale {
  const language = navigator.language.toLowerCase().split("-")[0];
  return language === "az" || language === "en" || language === "ru"
    ? language
    : "tr";
}

export function makeStrings(preference: AppPrefs["locale"]) {
  const locale = preference ?? systemLocale();
  const dictionary = dictionaries[locale] ?? dictionaries.tr ?? {};
  return {
    locale,
    t(key: string, values: Record<string, string | number> = {}): string {
      const raw = dictionary[key];
      let result = typeof raw === "string" ? raw : key;
      for (const [name, value] of Object.entries(values)) {
        result = result.replaceAll(`{${name}}`, String(value));
      }
      return result;
    },
  };
}

export type Strings = ReturnType<typeof makeStrings>;
