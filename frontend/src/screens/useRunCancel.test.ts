import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { Plan } from "../types";
import { useRunCancel } from "./useRunCancel";

const s = makeStrings("en");
const plan = { planId: "p1", opId: "rfi.log_add" } as unknown as Plan;

function setup(call: ReturnType<typeof vi.fn>, overrides: { plan?: Plan | null } = {}) {
  const setError = vi.fn();
  const onPreviewCancelled = vi.fn();
  const bridge = { call } as unknown as Bridge;
  const hook = renderHook(() =>
    useRunCancel({
      bridge,
      operationId: "rfi.log_add",
      plan: overrides.plan === undefined ? plan : overrides.plan,
      s,
      setError,
      onPreviewCancelled,
    }),
  );
  return { ...hook, setError, onPreviewCancelled };
}

describe("useRunCancel", () => {
  it("cancels the preview by operation and returns to the form", async () => {
    const call = vi.fn(async () => null);
    const { result, onPreviewCancelled } = setup(call);
    await act(() => result.current.cancelPreview());
    expect(call).toHaveBeenCalledWith("engine/cancel", { opId: "rfi.log_add" });
    expect(onPreviewCancelled).toHaveBeenCalledTimes(1);
    expect(result.current.cancelling).toBe(false);
  });

  it("still returns to the form when the preview already finished", async () => {
    const call = vi.fn(async () => {
      throw new Error("already finished");
    });
    const { result, onPreviewCancelled } = setup(call);
    await act(() => result.current.cancelPreview());
    expect(onPreviewCancelled).toHaveBeenCalledTimes(1);
  });

  it("cancels the apply by plan id", async () => {
    const call = vi.fn(async () => null);
    const { result, setError } = setup(call);
    await act(() => result.current.cancelApply());
    expect(call).toHaveBeenCalledWith("engine/cancel", { planId: "p1" });
    expect(setError).not.toHaveBeenCalled();
  });

  it("does nothing to apply without a plan", async () => {
    const call = vi.fn(async () => null);
    const { result } = setup(call, { plan: null });
    await act(() => result.current.cancelApply());
    expect(call).not.toHaveBeenCalled();
  });

  it("reports a failed apply cancel", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { result, setError } = setup(call);
    await act(() => result.current.cancelApply());
    expect(setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
  });
});
