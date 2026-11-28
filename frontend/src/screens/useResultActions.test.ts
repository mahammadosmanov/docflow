import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { OperationResult } from "../types";
import { useResultActions } from "./useResultActions";

const s = makeStrings("en");
const result = { journalId: "j1" } as unknown as OperationResult;

function setup(call: ReturnType<typeof vi.fn>, overrides: { result?: OperationResult | null } = {}) {
  const setError = vi.fn();
  const onBack = vi.fn();
  const bridge = { call } as unknown as Bridge;
  const hook = renderHook(() =>
    useResultActions({
      bridge,
      result: overrides.result === undefined ? result : overrides.result,
      s,
      setError,
      onBack,
    }),
  );
  return { ...hook, setError, onBack };
}

describe("useResultActions", () => {
  it("undoes the run by journal and goes back", async () => {
    const call = vi.fn(async () => null);
    const { result: hook, onBack, setError } = setup(call);
    await act(() => hook.current.undo());
    expect(call).toHaveBeenCalledWith("engine/undo", { journalId: "j1" });
    expect(onBack).toHaveBeenCalledTimes(1);
    expect(setError).not.toHaveBeenCalled();
  });

  it("reports a failed undo and stays on the screen", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { result: hook, onBack, setError } = setup(call);
    await act(() => hook.current.undo());
    expect(onBack).not.toHaveBeenCalled();
    expect(setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
  });
});
