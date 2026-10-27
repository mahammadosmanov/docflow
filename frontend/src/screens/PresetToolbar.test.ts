import { describe, expect, it } from "vitest";

import { makeStrings } from "../i18n";
import type { OperationDef } from "../types";
import { presetCapableValues, presetCommandError } from "./PresetToolbar";

const operation = {
  id: "test.op",
  schema: [
    { name: "copies", kind: "int" },
    { name: "duplex", kind: "bool" },
    { name: "paper", kind: "choice" },
    { name: "source", kind: "folder" },
    { name: "secret_token", kind: "secret" },
    { name: "transient", kind: "bool", remember: false },
  ],
} as unknown as OperationDef;

describe("presetCapableValues", () => {
  it("keeps only remembered bool, choice and int values", () => {
    const values = {
      copies: 2,
      duplex: true,
      paper: "A4",
      source: "/tmp/in",
      secret_token: "x",
      transient: true,
    };
    expect(presetCapableValues(operation, values)).toEqual({
      copies: 2,
      duplex: true,
      paper: "A4",
    });
  });

  it("ignores values that the schema does not describe", () => {
    expect(presetCapableValues(operation, { unknown: 1 })).toEqual({});
  });
});

describe("presetCommandError", () => {
  const s = makeStrings("en");

  it("explains a failed settings write in plain words", () => {
    const error = Object.assign(new Error("settings_write_failed"), {
      code: "settings_write_failed",
    });
    expect(presetCommandError(error, s)).toBe(s.t("error.preset.write"));
  });

  it("falls back to the general error translation for other failures", () => {
    expect(presetCommandError(new Error("something else"), s)).toBe("something else");
  });
});
