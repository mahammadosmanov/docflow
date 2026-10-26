import { describe, expect, it } from "vitest";

import type { ParamDef } from "../types";
import { persistableValues } from "./operationValues";

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
