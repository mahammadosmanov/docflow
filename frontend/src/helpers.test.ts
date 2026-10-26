import { describe, expect, it } from "vitest";

import { humanError, isPathSelection, parseRfiInputs } from "./helpers";
import { makeStrings } from "./i18n";

const s = makeStrings("en");

describe("parseRfiInputs", () => {
  it("splits on whitespace, commas, and semicolons, dropping duplicates", () => {
    expect(parseRfiInputs("RFI-1, RFI-2;RFI-1\nRFI-3")).toEqual(["RFI-1", "RFI-2", "RFI-3"]);
  });

  it("returns nothing for a non-string value", () => {
    expect(parseRfiInputs(undefined)).toEqual([]);
    expect(parseRfiInputs(["RFI-1"])).toEqual([]);
  });
});

describe("isPathSelection", () => {
  it("accepts an object with a selection id and display path", () => {
    expect(isPathSelection({ selectionId: "sel-1", displayPath: "/RFI" })).toBe(true);
  });

  it("rejects missing fields and non-objects", () => {
    expect(isPathSelection({ selectionId: "sel-1" })).toBe(false);
    expect(isPathSelection(null)).toBe(false);
    expect(isPathSelection("/RFI")).toBe(false);
  });
});

describe("humanError", () => {
  it("returns the localized text for a bare error key", () => {
    expect(humanError(new Error("error.watch.notReady"), s)).toBe(s.t("error.watch.notReady"));
  });

  it("appends the detail to the localized text of a keyed error", () => {
    expect(humanError(new Error("error.watch.notReady: disk full"), s)).toBe(
      `${s.t("error.watch.notReady")} (disk full)`,
    );
  });

  it("maps known authentication and rate-limit messages", () => {
    expect(humanError("invalid_credentials", s)).toBe(s.t("auth.err.credentials"));
    expect(humanError("rate limit exceeded", s)).toBe(s.t("auth.err.rateLimit"));
  });

  it("falls back to the raw message without the Error prefix", () => {
    expect(humanError(new Error("Error: something else"), s)).toBe("something else");
  });
});
