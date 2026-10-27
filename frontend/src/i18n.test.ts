import { afterEach, describe, expect, it, vi } from "vitest";

import { loadLocale, makeStrings, systemLocale } from "./i18n";

const setLanguage = (language: string) =>
  vi.spyOn(navigator, "language", "get").mockReturnValue(language);

afterEach(() => {
  vi.restoreAllMocks();
});

describe("systemLocale", () => {
  it("keeps the supported languages, ignoring region tags and case", () => {
    setLanguage("ru-RU");
    expect(systemLocale()).toBe("ru");
    setLanguage("AZ");
    expect(systemLocale()).toBe("az");
    setLanguage("en-GB");
    expect(systemLocale()).toBe("en");
  });

  it("falls back to Turkish for anything unsupported", () => {
    setLanguage("de-DE");
    expect(systemLocale()).toBe("tr");
  });
});

describe("loadLocale", () => {
  it("sets the document language so screen readers pronounce the text", async () => {
    await loadLocale("ru");
    expect(document.documentElement.lang).toBe("ru");
    await loadLocale("tr");
    expect(document.documentElement.lang).toBe("tr");
  });
});

describe("makeStrings", () => {
  it("returns the key itself when the catalog has no entry", async () => {
    await loadLocale("en");
    expect(makeStrings("en").t("definitely.missing.key")).toBe(
      "definitely.missing.key",
    );
  });

  it("replaces named placeholders in the returned text", async () => {
    await loadLocale("en");
    expect(makeStrings("en").t("definitely.missing.{count}", { count: 3 })).toBe(
      "definitely.missing.3",
    );
  });

  it("uses the requested locale once it is loaded", async () => {
    await loadLocale("tr");
    await loadLocale("en");
    expect(makeStrings("en").locale).toBe("en");
    expect(makeStrings("tr").locale).toBe("tr");
  });

  it("uses the system locale when no preference is set", () => {
    setLanguage("ru-RU");
    expect(makeStrings(null).locale).toBe("ru");
  });
});
