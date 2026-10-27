import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { SchemaField } from "./SchemaField";
import type { ParamDef } from "../types";

const param = {
  name: "destination",
  labelKey: "param.destination",
  kind: "text",
  required: true,
} as unknown as ParamDef;

describe("SchemaField invalid state", () => {
  afterEach(cleanup);

  it("marks the input invalid only when the parent says so", () => {
    const common = {
      bridge: {} as never,
      operationId: "op",
      param,
      value: "",
      values: {},
      s: makeStrings("en"),
      onChange: vi.fn(),
    };
    const { rerender } = render(<SchemaField {...common} />);
    expect(screen.getByRole("textbox")).not.toHaveAttribute("aria-invalid");
    rerender(<SchemaField {...common} invalid />);
    expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "true");
  });
});
