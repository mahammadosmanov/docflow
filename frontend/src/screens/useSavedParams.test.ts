import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Bridge } from "../lib/bridge";
import type { OperationDef, ParamDef } from "../types";
import { useSavedParams } from "./useSavedParams";

afterEach(cleanup);

const param = (name: string, kind: ParamDef["kind"] = "text"): ParamDef =>
  ({ name, kind, default: "", remember: true }) as unknown as ParamDef;

const operation = {
  id: "op.test",
  supportsPresets: false,
  schema: [param("source"), param("copies", "int")],
} as unknown as OperationDef;

const presetOperation = {
  ...operation,
  supportsPresets: true,
} as unknown as OperationDef;

function setup(
  saved: Record<string, unknown>,
  values: Record<string, unknown> = {},
  op: OperationDef = operation,
) {
  const call = vi.fn(async (method: string) =>
    method === "settings/params" ? saved : undefined,
  );
  const bridge = { call } as unknown as Bridge;
  const setValues = vi.fn();
  const hook = renderHook(
    (props: { values: Record<string, unknown> }) =>
      useSavedParams({
        bridge,
        operation: op,
        values: props.values,
        defaults: { source: "", copies: 1 },
        setValues,
      }),
    { initialProps: { values } },
  );
  return { call, setValues, hook };
}

describe("useSavedParams", () => {
  it("restores remembered values for an operation without presets", async () => {
    const { setValues, hook } = setup({ source: "/a", copies: 3 });
    await waitFor(() => expect(setValues).toHaveBeenCalled());
    const update = setValues.mock.calls[0][0] as (current: object) => object;
    expect(update({})).toEqual({ source: "/a", copies: 3 });
    expect(hook.result.current.previousValues).toBeNull();
  });

  it("keeps remembered values aside for operations that use presets", async () => {
    const { setValues, hook } = setup({ source: "/a" }, {}, presetOperation);
    await waitFor(() => expect(setValues).toHaveBeenCalled());
    const update = setValues.mock.calls[0][0] as (current: object) => object;
    expect(update({})).toEqual({});
    await waitFor(() =>
      expect(hook.result.current.previousValues).toEqual({ source: "/a" }),
    );
  });

  it("autosaves edited values after the debounce", async () => {
    vi.useFakeTimers();
    try {
      const { call, hook } = setup({ source: "/a", copies: 1 }, { source: "/a", copies: 1 });
      await act(async () => {});
      expect(call).toHaveBeenCalledWith("settings/params", { opId: "op.test" });
      hook.rerender({ values: { source: "/b", copies: 1 } });
      await act(async () => {
        vi.advanceTimersByTime(250);
      });
      expect(call).toHaveBeenCalledWith("settings/saveParams", {
        opId: "op.test",
        values: { source: "/b", copies: 1 },
      });
    } finally {
      vi.useRealTimers();
    }
  });
});
