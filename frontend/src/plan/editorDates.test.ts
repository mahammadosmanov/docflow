import { describe, expect, it } from "vitest";

import { formatEditorDate, parseEditorDate } from "./editorDates";

describe("parseEditorDate", () => {
  it("parses two- and four-digit years, ignoring case and outer spaces", () => {
    expect(parseEditorDate(" 05-Mar-26 ")).toEqual({ day: 5, month: 3, year: 2026 });
    expect(parseEditorDate("5-mar-2026")).toEqual({ day: 5, month: 3, year: 2026 });
  });

  it("accepts the leap day only in leap years", () => {
    expect(parseEditorDate("29-Feb-24")).toEqual({ day: 29, month: 2, year: 2024 });
    expect(parseEditorDate("29-Feb-26")).toBeNull();
  });

  it("rejects days outside the month, unknown months, and bad shapes", () => {
    expect(parseEditorDate("31-Apr-26")).toBeNull();
    expect(parseEditorDate("00-Jan-26")).toBeNull();
    expect(parseEditorDate("01-Foo-26")).toBeNull();
    expect(parseEditorDate("1/3/26")).toBeNull();
  });

  it("only accepts years from 2000 through 2068", () => {
    expect(parseEditorDate("01-Jan-00")).toEqual({ day: 1, month: 1, year: 2000 });
    expect(parseEditorDate("01-Jan-68")).toEqual({ day: 1, month: 1, year: 2068 });
    expect(parseEditorDate("01-Jan-99")).toBeNull();
    expect(parseEditorDate("01-Jan-1999")).toBeNull();
    expect(parseEditorDate("01-Jan-2069")).toBeNull();
  });
});

describe("formatEditorDate", () => {
  it("writes a zero-padded day and a two-digit year", () => {
    expect(formatEditorDate({ day: 5, month: 3, year: 2026 })).toBe("05-Mar-26");
  });

  it("round-trips through parseEditorDate", () => {
    const parts = parseEditorDate("12-Dec-31");
    expect(parts).not.toBeNull();
    expect(formatEditorDate(parts!)).toBe("12-Dec-31");
  });
});
