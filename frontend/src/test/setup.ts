import "@testing-library/jest-dom/vitest";

import { loadLocale } from "../i18n";

await Promise.all((["az", "en", "ru", "tr"] as const).map(loadLocale));
