import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type {
  PathSelection,
  RfiPrintCandidate,
  RfiPrintFindResult,
} from "../types";
import { useRfiPrintFinder } from "./useRfiPrintFinder";

const s = makeStrings("en");
const root: PathSelection = {
  selectionId: "root-1",
  displayPath: "/RFI",
  name: "RFI",
};
const candidate = (
  requested: string,
  selected = true,
): RfiPrintCandidate => ({
  requested,
  rfiCode: `CODE-${requested}`,
  documentType: "firstPage",
  displayPath: `/RFI/${requested}.pdf`,
  selection: { selectionId: requested, displayPath: "", name: "" },
  selected,
});
const result = (overrides: Partial<RfiPrintFindResult> = {}): RfiPrintFindResult => ({
  candidates: [candidate("1"), candidate("2", false)],
  resolutions: [
    {
      requested: "1",
      rfiCodes: ["CODE-1"],
      missingFirstPage: false,
      missingSketch: false,
      ambiguous: false,
    },
  ],
  visited: 10,
  truncated: false,
  ...overrides,
});

function setup(
  call: ReturnType<typeof vi.fn>,
  values: Record<string, unknown> = {
    rfi_system_root: root,
    include_first_page: true,
  },
  rfiInputs = ["1", "2"],
) {
  const bridge = { call } as unknown as Bridge;
  return renderHook(
    ({ inputs }) =>
      useRfiPrintFinder({ bridge, s, values, rfiInputs: inputs }),
    { initialProps: { inputs: rfiInputs } },
  );
}

describe("useRfiPrintFinder search guards", () => {
  it("asks for a system folder before searching", async () => {
    const call = vi.fn();
    const { result: hook } = setup(call, { include_first_page: true });
    await act(async () => hook.current.searchRfiDocuments());
    expect(hook.current.rfiFinderError).toBe(s.t("print.rfi.rootRequired"));
    expect(call).not.toHaveBeenCalled();
  });

  it("asks for at least one RFI number, counting excluded ones out", async () => {
    const call = vi.fn();
    const { result: hook } = setup(call);
    act(() => hook.current.setExcludedRfiInputs(["1", "2"]));
    await act(async () => hook.current.searchRfiDocuments());
    expect(hook.current.rfiFinderError).toBe(s.t("print.rfi.inputRequired"));
    expect(call).not.toHaveBeenCalled();
  });

  it("asks for a document type when neither first page nor sketch is chosen", async () => {
    const call = vi.fn();
    const { result: hook } = setup(call, { rfi_system_root: root });
    await act(async () => hook.current.searchRfiDocuments());
    expect(hook.current.rfiFinderError).toBe(s.t("print.rfi.typesRequired"));
    expect(call).not.toHaveBeenCalled();
  });
});

describe("useRfiPrintFinder search", () => {
  it("sends the included numbers and document types to the bridge", async () => {
    const call = vi.fn().mockResolvedValue(result());
    const { result: hook } = setup(call, {
      rfi_system_root: root,
      include_first_page: true,
      include_sketch: true,
    });
    act(() => hook.current.setExcludedRfiInputs(["2"]));
    await act(async () => hook.current.searchRfiDocuments());
    expect(call).toHaveBeenCalledWith("rfi/printFind", {
      root,
      numbers: ["1"],
      firstPage: true,
      sketch: true,
    });
  });

  it("stores candidates, resolutions, and the truncation flag", async () => {
    const call = vi.fn().mockResolvedValue(result({ truncated: true }));
    const { result: hook } = setup(call);
    await act(async () => hook.current.searchRfiDocuments());
    expect(hook.current.rfiPrintCandidates).toHaveLength(2);
    expect(hook.current.rfiPrintResolutions).toHaveLength(1);
    expect(hook.current.rfiFinderWasRun).toBe(true);
    expect(hook.current.rfiSearchTruncated).toBe(true);
    expect(hook.current.rfiFinderBusy).toBe(false);
  });

  it("shows a readable error when the search fails", async () => {
    const call = vi.fn().mockRejectedValue(new Error("boom"));
    const { result: hook } = setup(call);
    await act(async () => hook.current.searchRfiDocuments());
    expect(hook.current.rfiFinderError).not.toBe("");
    expect(hook.current.rfiFinderWasRun).toBe(false);
    expect(hook.current.rfiFinderBusy).toBe(false);
  });

  it("drops a late result after the finder was invalidated", async () => {
    let resolveSearch!: (value: RfiPrintFindResult) => void;
    const call = vi.fn(
      () => new Promise<RfiPrintFindResult>((resolve) => (resolveSearch = resolve)),
    );
    const { result: hook } = setup(call);
    let pending!: Promise<void>;
    act(() => {
      pending = hook.current.searchRfiDocuments();
    });
    act(() => hook.current.invalidateRfiFinder());
    await act(async () => {
      resolveSearch(result());
      await pending;
    });
    expect(hook.current.rfiPrintCandidates).toEqual([]);
    expect(hook.current.rfiFinderWasRun).toBe(false);
  });
});

describe("useRfiPrintFinder candidate selection", () => {
  it("selects or clears only the candidates that match the predicate", async () => {
    const call = vi.fn().mockResolvedValue(result());
    const { result: hook } = setup(call);
    await act(async () => hook.current.searchRfiDocuments());
    act(() =>
      hook.current.setRfiCandidateGroup((item) => item.requested === "2", true),
    );
    expect(hook.current.rfiPrintCandidates.map((item) => item.selected)).toEqual([
      true,
      true,
    ]);
    act(() =>
      hook.current.setRfiCandidateGroup((item) => item.requested === "1", false),
    );
    expect(hook.current.rfiPrintCandidates.map((item) => item.selected)).toEqual([
      false,
      true,
    ]);
  });
});
