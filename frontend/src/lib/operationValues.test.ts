import { describe, expect, it } from "vitest";

import type { ParamDef } from "../types";
import {
  isParamVisible,
  persistableValues,
  previewValuesFor,
} from "./operationValues";

const param = (name: string, extra: Partial<ParamDef> = {}): ParamDef => ({
  name,
  kind: "text",
  labelKey: name,
  default: null,
  required: false,
  choices: [],
  choiceLabels: {},
  visibleWhen: {},
  helpKey: null,
  ...extra,
});

describe("persistableValues", () => {
  it("drops transient, long-text, and secret values and keeps the rest", () => {
    const schema = [
      param("copies"),
      param("notes", { kind: "longText" }),
      param("token", { kind: "secret" }),
      param("files", { remember: false }),
    ];
    expect(
      persistableValues(schema, {
        copies: 2,
        notes: "draft",
        token: "x",
        files: ["a.pdf"],
        undeclared: true,
      }),
    ).toEqual({ copies: 2, undeclared: true });
  });
});

describe("previewValuesFor", () => {
  it("leaves other operations untouched", () => {
    expect(
      previewValuesFor("rfi.log_add", { workbook: "a.xlsx" }, { ran: true, candidates: [] }),
    ).toEqual({ workbook: "a.xlsx" });
  });

  it("drops finder-only inputs and takes files from selected candidates", () => {
    const result = previewValuesFor(
      "files.batch_print",
      {
        files: [],
        printer: "P1",
        rfi_system_root: "/r",
        rfi_numbers: "1",
        include_first_page: true,
        include_sketch: false,
      },
      {
        ran: true,
        candidates: [
          { selected: true, selection: "sel-a" },
          { selected: false, selection: "sel-b" },
        ],
      },
    );
    expect(result).toEqual({ files: ["sel-a"], printer: "P1" });
  });

  it("keeps typed files when the finder did not run", () => {
    expect(
      previewValuesFor(
        "files.batch_print",
        { files: ["typed"], rfi_numbers: "1" },
        { ran: false, candidates: [] },
      ),
    ).toEqual({ files: ["typed"] });
  });
});

describe("isParamVisible", () => {
  it("shows parameters without conditions", () => {
    expect(isParamVisible(param("copies"), {})).toBe(true);
  });

  it("shows a parameter only when every condition matches", () => {
    const gated = param("batch", { visibleWhen: { mode: "multi", fast: true } });
    expect(isParamVisible(gated, { mode: "multi", fast: true })).toBe(true);
    expect(isParamVisible(gated, { mode: "multi", fast: false })).toBe(false);
    expect(isParamVisible(gated, { mode: "single", fast: true })).toBe(false);
  });
});
