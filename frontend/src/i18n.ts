import azSource from "./locales/app_az.arb?raw";
import enSource from "./locales/app_en.arb?raw";
import ruSource from "./locales/app_ru.arb?raw";
import trSource from "./locales/app_tr.arb?raw";

import type { AppPrefs } from "./types";

type Locale = NonNullable<AppPrefs["locale"]>;
type Dictionary = Record<string, unknown>;

const dictionaries: Record<Locale, Dictionary> = {
  az: JSON.parse(azSource) as Dictionary,
  en: JSON.parse(enSource) as Dictionary,
  ru: JSON.parse(ruSource) as Dictionary,
  tr: JSON.parse(trSource) as Dictionary,
};

export function systemLocale(): Locale {
  const language = navigator.language.toLowerCase().split("-")[0];
  return language === "az" || language === "en" || language === "ru"
    ? language
    : "tr";
}

export function makeStrings(preference: AppPrefs["locale"]) {
  const locale = preference ?? systemLocale();
  const dictionary = dictionaries[locale] ?? dictionaries.tr;
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
