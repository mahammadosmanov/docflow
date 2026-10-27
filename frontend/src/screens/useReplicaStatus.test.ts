import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Bridge } from "../lib/bridge";
import type { ReplicaAutomationStatus } from "../types";
import { useReplicaStatus } from "./useReplicaStatus";

const status = { enabled: true, status: "idle" } as unknown as ReplicaAutomationStatus;

describe("useReplicaStatus", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("loads the status on mount and refreshes every five seconds", async () => {
    const call = vi.fn(async () => status);
    const bridge = { call } as unknown as Bridge;
    const { result } = renderHook(() => useReplicaStatus(bridge));
    await act(async () => {});
    expect(result.current.replicaStatus).toEqual(status);
    expect(call).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(5_000));
    expect(call).toHaveBeenCalledTimes(2);
  });

  it("keeps the last good status when a refresh fails", async () => {
    let fail = false;
    const call = vi.fn(async () => {
      if (fail) throw new Error("unavailable");
      return status;
    });
    const bridge = { call } as unknown as Bridge;
    const { result } = renderHook(() => useReplicaStatus(bridge));
    await act(async () => {});
    expect(result.current.replicaStatus).toEqual(status);
    fail = true;
    await act(() => vi.advanceTimersByTimeAsync(5_000));
    expect(result.current.replicaStatus).toEqual(status);
  });
});
