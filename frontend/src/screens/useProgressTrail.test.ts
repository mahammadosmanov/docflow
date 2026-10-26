import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useProgressTrail } from "./useProgressTrail";

const event = (phase: string, done = 0) => ({ phase, done, total: 0 });

describe("useProgressTrail", () => {
  it("replaces the last trail entry for a repeated phase and appends a new one", () => {
    const { result } = renderHook(() => useProgressTrail());
    act(() => result.current.receiveProgress(event("scan", 1)));
    act(() => result.current.receiveProgress(event("scan", 2)));
    act(() => result.current.receiveProgress(event("print")));
    expect(result.current.progressTrail).toEqual([event("scan", 2), event("print")]);
    expect(result.current.progress).toEqual(event("print"));
  });

  it("starts a fresh trail with a single event", () => {
    const { result } = renderHook(() => useProgressTrail());
    act(() => result.current.receiveProgress(event("scan")));
    act(() => result.current.startProgress(event("apply")));
    expect(result.current.progressTrail).toEqual([event("apply")]);
    expect(result.current.progress).toEqual(event("apply"));
  });
});
