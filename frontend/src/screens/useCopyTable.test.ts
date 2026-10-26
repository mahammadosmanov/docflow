import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { Plan } from "../types";
import { useCopyTable } from "./useCopyTable";

const s = makeStrings("en");
const plan = { planId: "p1", opId: "rfi.log_add" } as unknown as Plan;

function setup(call: ReturnType<typeof vi.fn>, currentPlan: Plan | null = plan) {
  const setError = vi.fn();
  const bridge = { call } as unknown as Bridge;
  const hook = renderHook(() => useCopyTable({ bridge, plan: currentPlan, s, setError }));
  return { ...hook, setError };
}

describe("useCopyTable", () => {
  it("copies the table for the plan and reports the row count", async () => {
    const call = vi.fn(async () => ({ count: 7 }));
    const { result, setError } = setup(call);
    await act(() => result.current.copyTable("Sheet1"));
    expect(call).toHaveBeenCalledWith("engine/copyTable", { planId: "p1", sheet: "Sheet1" });
    expect(setError).toHaveBeenLastCalledWith(s.t("rfi.editor.copied", { count: 7 }));
    expect(result.current.copyingSheet).toBeNull();
  });

  it("does nothing without a plan", async () => {
    const call = vi.fn(async () => ({ count: 0 }));
    const { result } = setup(call, null);
    await act(() => result.current.copyTable());
    expect(call).not.toHaveBeenCalled();
  });

  it("reports a failed copy and clears the in-flight sheet", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { result, setError } = setup(call);
    await act(() => result.current.copyTable());
    expect(setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
    expect(result.current.copyingSheet).toBeNull();
  });
});
