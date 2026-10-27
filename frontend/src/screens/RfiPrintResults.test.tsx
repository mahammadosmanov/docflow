import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { RfiPrintCandidate, RfiPrintResolution } from "../types";
import { RfiPrintResults } from "./RfiPrintResults";

const resolution = (requested: string, extra: Partial<RfiPrintResolution> = {}) =>
  ({
    requested,
    ambiguous: false,
    rfiCodes: [],
    missingFirstPage: false,
    missingSketch: false,
    ...extra,
  }) as RfiPrintResolution;

const candidate = (requested: string, selected: boolean) =>
  ({ requested, selected, rfiCode: `PRJ-RFI-CIV--1` }) as RfiPrintCandidate;

describe("RfiPrintResults", () => {
  afterEach(cleanup);

  it("selects every candidate of a request that is not fully selected yet", () => {
    const onSetGroup = vi.fn();
    render(
      <RfiPrintResults
        resolutions={[resolution("RFI-1")]}
        candidates={[candidate("RFI-1", true), candidate("RFI-1", false)]}
        truncated={false}
        wasRun
        onSetGroup={onSetGroup}
        s={makeStrings("en")}
      />,
    );
    fireEvent.click(screen.getAllByRole("button")[0]);
    const [predicate, selected] = onSetGroup.mock.calls[0];
    expect(selected).toBe(true);
    expect(predicate(candidate("RFI-1", false))).toBe(true);
    expect(predicate(candidate("RFI-2", false))).toBe(false);
  });

  it("clears a request whose candidates are all selected", () => {
    const onSetGroup = vi.fn();
    render(
      <RfiPrintResults
        resolutions={[resolution("RFI-1")]}
        candidates={[candidate("RFI-1", true)]}
        truncated={false}
        wasRun
        onSetGroup={onSetGroup}
        s={makeStrings("en")}
      />,
    );
    fireEvent.click(screen.getAllByRole("button")[0]);
    expect(onSetGroup.mock.calls[0][1]).toBe(false);
  });

  it("announces an ambiguous request with the candidate RFI codes", () => {
    render(
      <RfiPrintResults
        resolutions={[resolution("RFI-7", { ambiguous: true, rfiCodes: ["CIV-1", "MEC-2"] })]}
        candidates={[]}
        truncated={false}
        wasRun={false}
        onSetGroup={vi.fn()}
        s={makeStrings("en")}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("CIV-1, MEC-2");
  });
});
