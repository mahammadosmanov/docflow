import { describe, expect, it } from "vitest";

import { applyEditorValue } from "./editorRules";

describe("applyEditorValue", () => {
  it("writes the value and applies the auto-fill preset for the matching key", () => {
    const editor = {
      autoFill: {
        field: "status",
        values: { closed: { reviewed: true, owner: "" } },
      },
    };
    const row: Record<string, unknown> = { status: "" };
    applyEditorValue(editor, row, "status", "closed");
    expect(row).toEqual({ status: "closed", reviewed: true, owner: "" });
  });

  it("leaves other keys alone when auto-fill names a different field", () => {
    const editor = {
      autoFill: { field: "status", values: { closed: { reviewed: true } } },
    };
    const row: Record<string, unknown> = {};
    applyEditorValue(editor, row, "note", "closed");
    expect(row).toEqual({ note: "closed" });
  });

  it("sets the regex target when the pattern matches, case-insensitively", () => {
    const editor = {
      regexFill: [
        { source: "code", pattern: "^abc", target: "kind", value: "ABC" },
      ],
    };
    const row: Record<string, unknown> = {};
    applyEditorValue(editor, row, "code", "ABCD-1");
    expect(row).toEqual({ code: "ABCD-1", kind: "ABC" });
  });

  it("ignores a regex rule whose pattern does not match or is invalid", () => {
    const editor = {
      regexFill: [
        { source: "code", pattern: "^zzz", target: "kind", value: "Z" },
        { source: "code", pattern: "([", target: "other", value: "X" },
      ],
    };
    const row: Record<string, unknown> = {};
    applyEditorValue(editor, row, "code", "ABC");
    expect(row).toEqual({ code: "ABC" });
  });
});
