import { describe, expect, it } from "vitest";

import {
  applyEditorValue,
  filterRowIndices,
  duplicateRowAt,
  moveRowAt,
  removeRowAt,
} from "./editorRules";

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

describe("row operations", () => {
  const rows = [
    { _rowId: "a", rfiNo: "1" },
    { _rowId: "b", rfiNo: "1" },
    { _rowId: "c", rfiNo: "2" },
  ];

  it("copies a row after its group and gives the copy a new id", () => {
    const next = duplicateRowAt(rows, 0, "rfiNo", 7);
    expect(next.map((row) => row._rowId)).toEqual(["a", "b", "a-copy-7", "c"]);
    expect(rows).toHaveLength(3);
  });

  it("removes a row only while another row in its group remains", () => {
    expect(removeRowAt(rows, 0, "rfiNo")?.map((row) => row._rowId)).toEqual(["b", "c"]);
    expect(removeRowAt(rows, 2, "rfiNo")).toBeNull();
  });

  it("swaps a row with its neighbour and refuses to leave the list", () => {
    expect(moveRowAt(rows, 1, -1)?.map((row) => row._rowId)).toEqual(["b", "a", "c"]);
    expect(moveRowAt(rows, 0, -1)).toBeNull();
    expect(moveRowAt(rows, 2, 1)).toBeNull();
  });
});

describe("filterRowIndices", () => {
  const rows = [
    { currentStatus: "open", proposedStatus: "open", note: "Alpha" },
    { currentStatus: "open", proposedStatus: "closed", note: "Beta" },
    { currentStatus: "open", proposedStatus: "open", enabled: true, note: "Gamma" },
    { disposition: "manual_review", note: "Delta" },
    { reasonKey: "reason.x", note: "Epsilon", code: 42 },
  ];

  it("returns every index in all mode without a search", () => {
    expect(filterRowIndices(rows, "all", "")).toEqual([0, 1, 2, 3, 4]);
  });

  it("keeps rows whose proposed status differs or that are enabled in changes mode", () => {
    expect(filterRowIndices(rows, "changes", "")).toEqual([1, 2]);
  });

  it("keeps manual-review, reason-coded, unselectable, or disabled rows in review mode", () => {
    expect(filterRowIndices(rows, "review", "")).toEqual([3, 4]);
  });

  it("matches the search against string and number fields only", () => {
    expect(filterRowIndices(rows, "all", "beta")).toEqual([1]);
    expect(filterRowIndices(rows, "all", "42")).toEqual([4]);
    expect(filterRowIndices(rows, "all", "true")).toEqual([]);
  });

  it("applies the mode and the search together", () => {
    expect(filterRowIndices(rows, "changes", "gamma")).toEqual([2]);
    expect(filterRowIndices(rows, "review", "beta")).toEqual([]);
  });
});
