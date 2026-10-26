import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import { usePrintQueue } from "./usePrintQueue";

const summary = { queueId: "q1", resumable: 2 };

function fakeBridge(handlers: Record<string, () => unknown>) {
  return {
    call: vi.fn(async (method: string) => {
      const handler = handlers[method];
      if (!handler) throw new Error(`unexpected ${method}`);
      return handler();
    }),
  } as unknown as Bridge & { call: ReturnType<typeof vi.fn> };
}

const s = makeStrings("en");

describe("usePrintQueue", () => {
  it("loads the latest queue for batch print only", async () => {
    const bridge = fakeBridge({ "engine/printQueueLatest": () => summary });
    const { result } = renderHook(() =>
      usePrintQueue({ bridge, operationId: "files.batch_print", s, setError: vi.fn(), setStage: vi.fn(), installPlan: vi.fn() }),
    );
    await waitFor(() => expect(result.current.latestPrintQueue).toEqual(summary));

    const other = fakeBridge({});
    renderHook(() =>
      usePrintQueue({ bridge: other, operationId: "files.scan", s, setError: vi.fn(), setStage: vi.fn(), installPlan: vi.fn() }),
    );
    expect(other.call).not.toHaveBeenCalled();
  });

  it("clears the summary after a discard", async () => {
    const bridge = fakeBridge({
      "engine/printQueueLatest": () => summary,
      "engine/printQueueDiscard": () => null,
    });
    const { result } = renderHook(() =>
      usePrintQueue({ bridge, operationId: "files.batch_print", s, setError: vi.fn(), setStage: vi.fn(), installPlan: vi.fn() }),
    );
    await waitFor(() => expect(result.current.latestPrintQueue).toEqual(summary));
    await act(() => result.current.discardPrintQueue("q1"));
    expect(result.current.latestPrintQueue).toBeNull();
  });

  it("returns to the form with an error when resume fails", async () => {
    const bridge = fakeBridge({
      "engine/printQueueLatest": () => null,
      "engine/printQueueResume": () => {
        throw new Error("error.print.queueSourceReselectionRequired");
      },
    });
    const setError = vi.fn();
    const setStage = vi.fn();
    const installPlan = vi.fn();
    const { result } = renderHook(() =>
      usePrintQueue({ bridge, operationId: "files.batch_print", s, setError, setStage, installPlan }),
    );
    await act(() => result.current.resumePrintQueue("q1"));
    expect(setStage.mock.calls).toEqual([["previewing"], ["form"]]);
    expect(installPlan).not.toHaveBeenCalled();
    expect(setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
  });
});
