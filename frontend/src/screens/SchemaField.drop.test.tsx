import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { NativeDrop } from "../lib/platform";
import type { ParamDef } from "../types";
import { SchemaField } from "./SchemaField";

const listeners = new Set<(drop: NativeDrop) => void>();

vi.mock("../lib/platform", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/platform")>()),
  listenNativeDrop: (listener: (drop: NativeDrop) => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
}));

const s = makeStrings("en");
const selection = {
  selectionId: "dropped-1",
  displayPath: "/Users/demo/Scans",
  name: "Scans",
};

const folderParam: ParamDef = {
  name: "source",
  kind: "folder",
  labelKey: "param.source",
  default: null,
  required: true,
  choices: [],
  choiceLabels: {},
  visibleWhen: {},
  helpKey: null,
};

function emit(drop: NativeDrop) {
  act(() => listeners.forEach((listener) => listener(drop)));
}

describe("SchemaField native folder drop", () => {
  afterEach(() => {
    cleanup();
    listeners.clear();
  });

  it("claims a drop that lands on the field without sending any path", async () => {
    const call = vi.fn(async () => [selection]);
    const onChange = vi.fn();
    render(
      <SchemaField
        bridge={{ call } as unknown as Bridge}
        operationId="rfi.signed_scans"
        param={folderParam}
        value={null}
        values={{}}
        s={s}
        onChange={onChange}
      />,
    );
    const input = screen.getByRole("textbox");
    const zone = input.closest(".path-input");

    emit({ type: "over", target: input });
    expect(zone).toHaveClass("drop-active");

    emit({ type: "drop", target: input });
    expect(zone).not.toHaveClass("drop-active");
    await waitFor(() => expect(onChange).toHaveBeenCalledWith(selection));
    expect(call).toHaveBeenCalledWith("paths/select", {
      opId: "rfi.signed_scans",
      paramName: "source",
      fromDrop: true,
    });
  });

  it("ignores a drop that lands elsewhere in the window", () => {
    const call = vi.fn();
    render(
      <SchemaField
        bridge={{ call } as unknown as Bridge}
        operationId="rfi.signed_scans"
        param={folderParam}
        value={null}
        values={{}}
        s={s}
        onChange={vi.fn()}
      />,
    );
    emit({ type: "drop", target: document.body });
    expect(call).not.toHaveBeenCalled();
  });
});
