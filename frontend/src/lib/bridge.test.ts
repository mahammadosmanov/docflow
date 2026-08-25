import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  Channel: class FakeChannel<T> {
    onmessage: (message: T) => void = () => undefined;
  },
}));

import { TauriBridge } from "./bridge";
import type { ProgressEvent } from "../types";

describe("TauriBridge", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("starts lazily and routes calls and progress through Tauri invoke", async () => {
    const bridge = new TauriBridge();
    const progress: ProgressEvent[] = [];
    invokeMock.mockResolvedValue({ planId: "plan-1" });

    await bridge.start();
    expect(invokeMock).not.toHaveBeenCalled();

    const response = bridge.call(
      "engine/preview",
      { opId: "rfi.process_scans", values: {} },
      (event) => progress.push(event),
    );

    expect(invokeMock).toHaveBeenCalledTimes(1);
    const [command, payload] = invokeMock.mock.calls[0] as [
      string,
      {
        request: {
          method: string;
          params: Record<string, unknown>;
        };
        onProgress: { onmessage: (event: ProgressEvent) => void };
      },
    ];
    expect(command).toBe("frontend_call");
    expect(payload.request.method).toBe("engine/preview");
    expect(payload.request.params).toEqual({
      opId: "rfi.process_scans",
      values: {},
    });

    payload.onProgress.onmessage({ phase: "scan", done: 1, total: 2 });
    expect(progress).toEqual([{ phase: "scan", done: 1, total: 2 }]);
    await expect(response).resolves.toEqual({ planId: "plan-1" });
  });
});
