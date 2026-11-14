import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { Plan } from "../types";
import { useRunCancel } from "./useRunCancel";

const s = makeStrings("en");
const plan = { planId: "p1", opId: "rfi.log_add" } as unknown as Plan;

function setup(
  call: ReturnType<typeof vi.fn>,
  overrides: { plan?: Plan | null; twoStageCancel?: boolean } = {},
) {
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
      twoStageCancel: overrides.twoStageCancel ?? false,
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

  it("first apply cancel pauses after the current file when two-stage cancel is on", async () => {
    const call = vi.fn(async () => null);
    const { result, setError } = setup(call, { twoStageCancel: true });
    await act(() => result.current.cancelApply());
    expect(call).toHaveBeenCalledWith("engine/cancel", { planId: "p1" });
    expect(result.current.pausing).toBe(true);
    expect(result.current.cancelling).toBe(false);
    expect(setError).not.toHaveBeenCalled();
  });

  it("second apply cancel stops the active job when two-stage cancel is on", async () => {
    const call = vi.fn(async () => null);
    const { result } = setup(call, { twoStageCancel: true });
    await act(() => result.current.cancelApply());
    await act(() => result.current.cancelApply());
    expect(call).toHaveBeenCalledTimes(2);
    expect(result.current.pausing).toBe(true);
    expect(result.current.cancelling).toBe(true);
  });

  it("keeps the one-stage apply cancel when two-stage cancel is off", async () => {
    const call = vi.fn(async () => null);
    const { result } = setup(call);
    await act(() => result.current.cancelApply());
    expect(result.current.cancelling).toBe(true);
    expect(result.current.pausing).toBe(false);
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
