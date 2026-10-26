import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { OperationResult, Plan } from "../types";
import { useApplyRun } from "./useApplyRun";

const s = makeStrings("en");
const plan = { planId: "p1", opId: "rfi.log_add", warnings: [] } as unknown as Plan;
const rows = [{ code: "A" }];

function setup(
  call: ReturnType<typeof vi.fn>,
  overrides: { plan?: Plan | null; editorDirty?: boolean; opId?: string } = {},
) {
  const screen = {
    setError: vi.fn(),
    setPlan: vi.fn(),
    setEditorRows: vi.fn(),
    setEditorDirty: vi.fn(),
    setStage: vi.fn(),
    setResult: vi.fn(),
    setCancelling: vi.fn(),
  };
  const progress = { startProgress: vi.fn(), receiveProgress: vi.fn() };
  const updateScanPreview = vi.fn(async () => undefined);
  const hook = renderHook(() =>
    useApplyRun({
      bridge: { call } as unknown as Bridge,
      operationId: overrides.opId ?? "rfi.log_add",
      plan: overrides.plan === undefined ? plan : overrides.plan,
      editorDirty: overrides.editorDirty ?? false,
      editorRows: rows,
      s,
      screen,
      progress,
      updateScanPreview,
    }),
  );
  return { ...hook, screen, progress, updateScanPreview };
}

describe("useApplyRun", () => {
  it("applies the plan as it is when the editor is clean", async () => {
    const result = { journalId: "j1" } as unknown as OperationResult;
    const call = vi.fn(async () => result);
    const { result: hook, screen } = setup(call);
    await act(() => hook.current.apply());
    expect(call).toHaveBeenCalledWith("engine/apply", { opId: "rfi.log_add", planId: "p1" }, expect.any(Function));
    expect(screen.setResult).toHaveBeenCalledWith(result);
    expect(screen.setStage.mock.calls).toEqual([["applying"], ["result"]]);
    expect(screen.setCancelling).toHaveBeenLastCalledWith(false);
  });

  it("revises the plan first when the editor has unsaved rows", async () => {
    const revised = { planId: "p2", opId: "rfi.log_add", warnings: [], meta: {} } as unknown as Plan;
    const call = vi.fn(async (method: string) => (method === "engine/revise" ? revised : {}));
    const { result: hook, screen } = setup(call, { editorDirty: true });
    await act(() => hook.current.apply());
    expect(call).toHaveBeenNthCalledWith(1, "engine/revise", { planId: "p1", rows });
    expect(call).toHaveBeenNthCalledWith(2, "engine/apply", { opId: "rfi.log_add", planId: "p2" }, expect.any(Function));
    expect(screen.setPlan).toHaveBeenCalledWith(revised);
    expect(screen.setEditorDirty).toHaveBeenCalledWith(false);
  });

  it("stops at preview when the revision adds a warning", async () => {
    const revised = {
      planId: "p2",
      opId: "rfi.log_add",
      warnings: [{ key: "k", subject: "new" }],
      meta: {},
    } as unknown as Plan;
    const call = vi.fn(async () => revised);
    const { result: hook, screen } = setup(call, { editorDirty: true });
    await act(() => hook.current.apply());
    expect(call).toHaveBeenCalledTimes(1);
    expect(screen.setError).toHaveBeenLastCalledWith(s.t("rfi.editor.reviewWarnings"));
    expect(screen.setStage).toHaveBeenLastCalledWith("preview");
  });

  it("hands fast-scan edits to the preview re-run instead of applying", async () => {
    const call = vi.fn();
    const { result: hook, updateScanPreview } = setup(call, {
      editorDirty: true,
      opId: "rfi.process_scans_fast_covers",
      plan: { planId: "p1", opId: "rfi.process_scans_fast_covers", warnings: [] } as unknown as Plan,
    });
    await act(() => hook.current.apply());
    expect(updateScanPreview).toHaveBeenCalledTimes(1);
    expect(call).not.toHaveBeenCalled();
  });

  it("returns to preview with the error when apply fails", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { result: hook, screen } = setup(call);
    await act(() => hook.current.apply());
    expect(screen.setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
    expect(screen.setStage).toHaveBeenLastCalledWith("preview");
    expect(screen.setCancelling).toHaveBeenLastCalledWith(false);
  });
});
