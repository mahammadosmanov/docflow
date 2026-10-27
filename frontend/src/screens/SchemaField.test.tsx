import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { ParamDef } from "../types";
import { SchemaField } from "./SchemaField";

const s = makeStrings("en");
const bridge = { call: vi.fn() } as unknown as Bridge;

const intParam = (overrides: Partial<ParamDef> = {}): ParamDef => ({
  name: "copies",
  kind: "int",
  labelKey: "param.print.copies",
  default: 1,
  required: true,
  choices: [],
  choiceLabels: {},
  visibleWhen: {},
  helpKey: null,
  ...overrides,
});

describe("SchemaField integer inputs", () => {
  afterEach(cleanup);

  it("passes the backend bounds to the number input", () => {
    render(
      <SchemaField
        bridge={bridge}
        operationId="files.batch_print"
        param={intParam({ min: 1, max: 99 })}
        value={1}
        values={{}}
        s={s}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByRole("spinbutton");
    expect(input).toHaveAttribute("min", "1");
    expect(input).toHaveAttribute("max", "99");
  });

  it("leaves the number input unbounded when the backend sends no range", () => {
    render(
      <SchemaField
        bridge={bridge}
        operationId="files.batch_print"
        param={intParam({ name: "other" })}
        value={1}
        values={{}}
        s={s}
        onChange={vi.fn()}
      />,
    );
    const input = screen.getByRole("spinbutton");
    expect(input).not.toHaveAttribute("min");
    expect(input).not.toHaveAttribute("max");
  });
});
