import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { Plan } from "../types";
import { useScanPreviewUpdate } from "./useScanPreviewUpdate";

const s = makeStrings("en");
const fastPlan = { planId: "p1", opId: "rfi.process_scans_fast_covers" } as unknown as Plan;
const rows = [{ code: "AZE-SUN-INT-QMS-RFI-CIV-0001" }];

function run(overrides: Partial<Parameters<typeof useScanPreviewUpdate>[0]>, call: Bridge["call"]) {
  const args = {
    bridge: { call } as unknown as Bridge,
    plan: fastPlan,
    editorDirty: true,
    editorRows: rows,
    s,
    setError: vi.fn(),
    onRevised: vi.fn(),
    ...overrides,
  };
  return { args, ...renderHook(() => useScanPreviewUpdate(args)) };
}

describe("useScanPreviewUpdate", () => {
  it("does nothing for a plan that is not the fast cover scan", async () => {
    const call = vi.fn();
    const { result } = run({ plan: { planId: "p2", opId: "files.batch_print" } as unknown as Plan }, call as unknown as Bridge["call"]);
    await act(() => result.current.updateScanPreview());
    expect(call).not.toHaveBeenCalled();
  });

  it("does nothing when the editor has no unsaved changes", async () => {
    const call = vi.fn();
    const { result } = run({ editorDirty: false }, call as unknown as Bridge["call"]);
    await act(() => result.current.updateScanPreview());
    expect(call).not.toHaveBeenCalled();
  });

  it("revises the plan with the edited rows and hands the result back", async () => {
    const revised = { planId: "p1", opId: "rfi.process_scans_fast_covers" } as unknown as Plan;
    const call = vi.fn(async () => revised);
    const { args, result } = run({}, call as unknown as Bridge["call"]);
    await act(() => result.current.updateScanPreview());
    expect(call).toHaveBeenCalledWith("engine/revise", { planId: "p1", rows });
    expect(args.onRevised).toHaveBeenCalledWith(revised);
    expect(result.current.isUpdatingScanPreview).toBe(false);
  });

  it("reports a failed revision and keeps the current plan", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { args, result } = run({}, call as unknown as Bridge["call"]);
    await act(() => result.current.updateScanPreview());
    expect(args.onRevised).not.toHaveBeenCalled();
    expect(args.setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
    expect(result.current.isUpdatingScanPreview).toBe(false);
  });
});
