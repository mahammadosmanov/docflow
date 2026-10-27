import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import {
  BACKEND_MESSAGE_KEYS,
  formatDate,
  humanError,
  isDuplicatePresetError,
  isPathSelection,
  parseRfiInputs,
} from "./helpers";
import { loadLocale, makeStrings } from "./i18n";

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
  beforeAll(async () => {
    await loadLocale("en");
  });

  it("translates plain backend messages that have a catalog key", () => {
    expect(humanError(new Error("Error: print plan has no rows"), s)).toBe(
      "The print plan has no rows.",
    );
  });

  it("translates backend path error codes instead of showing the English message", () => {
    const error = Object.assign(new Error("path traversal is not allowed"), {
      code: "path_traversal",
    });
    expect(humanError(error, s)).toBe(
      "The path tries to leave its folder, so the action was refused.",
    );
  });

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

describe("isDuplicatePresetError", () => {
  it("recognises duplicate-preset messages in their known forms", () => {
    expect(isDuplicatePresetError(new Error("error.preset.duplicate"))).toBe(true);
    expect(isDuplicatePresetError("duplicate preset")).toBe(true);
    expect(isDuplicatePresetError("that preset already exists")).toBe(true);
  });

  it("does not match unrelated errors", () => {
    expect(isDuplicatePresetError("disk full")).toBe(false);
  });
});

describe("formatDate", () => {
  it("returns an unparseable value unchanged", () => {
    expect(formatDate("not a date", "en")).toBe("not a date");
  });

  it("formats a valid timestamp with the requested locale", () => {
    const value = "2026-03-05T12:00:00Z";
    const expected = new Intl.DateTimeFormat("en", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(value));
    expect(formatDate(value, "en")).toBe(expected);
  });
});

describe("BACKEND_MESSAGE_KEYS", () => {
  it("only maps messages that the backend still emits, so the table cannot go stale", () => {
    const rustSource = (dir: string): string =>
      readdirSync(dir)
        .map((name) => {
          const path = join(dir, name);
          if (statSync(path).isDirectory()) return rustSource(path);
          return name.endsWith(".rs") ? readFileSync(path, "utf8") : "";
        })
        .join("\n");
    const source = rustSource(resolve(process.cwd(), "src-tauri/src"));
    const stale = Object.keys(BACKEND_MESSAGE_KEYS).filter(
      (message) => !source.includes(`"${message}"`),
    );
    expect(stale).toEqual([]);
  });
});
