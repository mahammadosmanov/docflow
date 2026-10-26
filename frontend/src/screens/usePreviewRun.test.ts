import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { Plan } from "../types";
import { usePreviewRun } from "./usePreviewRun";

const s = makeStrings("en");
const operation = { id: "rfi.log_add", schema: [], supportsPresets: false };
const finder = { ran: false, candidates: [] };

function setup(call: ReturnType<typeof vi.fn>) {
  const screen = {
    setError: vi.fn(),
    setStage: vi.fn(),
    setPreviousValues: vi.fn(),
    installPlan: vi.fn(),
  };
  const progress = { startProgress: vi.fn(), receiveProgress: vi.fn() };
  const lastSaved = { current: "" };
  const hook = renderHook(() =>
    usePreviewRun({
      bridge: { call } as unknown as Bridge,
      operation,
      values: {},
      lastSaved,
      finder,
      s,
      screen,
      progress,
    }),
  );
  return { ...hook, screen, progress, lastSaved };
}

describe("usePreviewRun", () => {
  it("saves the parameters, requests a preview, and installs the plan", async () => {
    const plan = { planId: "p1" } as unknown as Plan;
    const call = vi.fn(async (method: string) => (method === "engine/preview" ? plan : null));
    const { result, screen, progress, lastSaved } = setup(call);
    await act(() => result.current.preview());
    expect(call.mock.calls.map(([method]) => method)).toEqual(["settings/saveParams", "engine/preview"]);
    expect(call).toHaveBeenCalledWith("engine/preview", { opId: "rfi.log_add", values: {} }, progress.receiveProgress);
    expect(screen.installPlan).toHaveBeenCalledWith(plan);
    expect(screen.setStage.mock.calls).toEqual([["previewing"]]);
    expect(lastSaved.current).toBe("{}");
  });

  it("returns to the form with the error when the preview fails", async () => {
    const call = vi.fn(async (method: string) => {
      if (method === "engine/preview") throw new Error("error.print.queueSourceReselectionRequired");
      return null;
    });
    const { result, screen } = setup(call);
    await act(() => result.current.preview());
    expect(screen.installPlan).not.toHaveBeenCalled();
    expect(screen.setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
    expect(screen.setStage).toHaveBeenLastCalledWith("form");
  });
});
