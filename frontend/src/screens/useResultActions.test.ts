import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { OperationResult, Plan } from "../types";
import { useResultActions } from "./useResultActions";

const s = makeStrings("en");
const plan = { planId: "p1", opId: "rfi.log_add" } as unknown as Plan;
const result = { journalId: "j1" } as unknown as OperationResult;

function setup(call: ReturnType<typeof vi.fn>, overrides: { result?: OperationResult | null } = {}) {
  const setError = vi.fn();
  const onBack = vi.fn();
  const onPrepareRfiReply = vi.fn();
  const bridge = { call } as unknown as Bridge;
  const hook = renderHook(() =>
    useResultActions({
      bridge,
      plan,
      result: overrides.result === undefined ? result : overrides.result,
      s,
      setError,
      onBack,
      onPrepareRfiReply,
    }),
  );
  return { ...hook, setError, onBack, onPrepareRfiReply };
}

describe("useResultActions", () => {
  it("prepares an RFI reply from the plan and journal", async () => {
    const reply = { subject: "Re" } as unknown as Plan;
    const call = vi.fn(async () => reply);
    const { result: hook, onPrepareRfiReply } = setup(call);
    await act(() => hook.current.prepareRfiReply());
    expect(call).toHaveBeenCalledWith("engine/rfiReplyPreview", { planId: "p1", journalId: "j1" });
    expect(onPrepareRfiReply).toHaveBeenCalledWith(reply);
  });

  it("does not prepare a reply without a journal", async () => {
    const call = vi.fn(async () => null);
    const { result: hook } = setup(call, { result: null });
    await act(() => hook.current.prepareRfiReply());
    expect(call).not.toHaveBeenCalled();
  });

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
