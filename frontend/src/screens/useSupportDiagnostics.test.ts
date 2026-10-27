import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import { useSupportDiagnostics } from "./useSupportDiagnostics";

const s = makeStrings("en");
const bundle = { fileName: "diagnostics.json", content: "{}", byteLength: 2 };

function setup(call: ReturnType<typeof vi.fn>) {
  const setError = vi.fn();
  const hook = renderHook(() =>
    useSupportDiagnostics({ bridge: { call } as unknown as Bridge, s, setError }),
  );
  return { ...hook, setError };
}

describe("useSupportDiagnostics", () => {
  it("previews the bundle returned by the bridge", async () => {
    const call = vi.fn(async () => bundle);
    const { result } = setup(call);
    await act(() => result.current.previewDiagnostics());
    expect(call).toHaveBeenCalledWith("support/diagnostics");
    expect(result.current.diagnostics).toEqual(bundle);
    expect(result.current.diagnosticsBusy).toBe(false);
  });

  it("reports a failed preview and stays idle", async () => {
    const call = vi.fn(async () => {
      throw new Error("error.print.queueSourceReselectionRequired");
    });
    const { result, setError } = setup(call);
    await act(() => result.current.previewDiagnostics());
    expect(setError).toHaveBeenLastCalledWith(s.t("error.print.queueSourceReselectionRequired"));
    expect(result.current.diagnosticsBusy).toBe(false);
  });

  it("copies nothing until a bundle has been previewed", async () => {
    const call = vi.fn(async () => null);
    const { result } = setup(call);
    await act(() => result.current.copyDiagnostics());
    expect(call).not.toHaveBeenCalled();
  });

  it("copies the previewed content and reports success", async () => {
    const call = vi.fn(async (method: string) => (method === "support/diagnostics" ? bundle : null));
    const { result } = setup(call);
    await act(() => result.current.previewDiagnostics());
    await act(() => result.current.copyDiagnostics());
    expect(call).toHaveBeenLastCalledWith("support/copyDiagnostics", { content: "{}" });
    expect(result.current.diagnosticsStatus).toBe(s.t("prefs.support.copied"));
  });
});
