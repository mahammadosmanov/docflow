import { describe, expect, it } from "vitest";

import type { Plan, Step } from "../types";
import {
  MAX_WORKBOOK_CELL_PREVIEWS,
  canonicalRfiInText,
  summarizeWorkbookCellChanges,
} from "./workbookCellChanges";

const CODE = "AZE-SUN-INT-QMS-RFI-CIV-0008";

const planWith = (steps: Step[], meta: Record<string, unknown> = {}) =>
  ({ planId: "p", opId: "op", steps, warnings: [], meta }) as unknown as Plan;

const syncStep = (
  updates: Record<string, Record<string, unknown>>,
  workbookKind = "rfiLog",
) =>
  ({
    kind: "workbook_update",
    meta: { syncStep: { workbookKind, updates } },
  }) as unknown as Step;

describe("canonicalRfiInText", () => {
  it("returns the canonical code when it stands alone in text", () => {
    expect(canonicalRfiInText(`Re: ${CODE} please`)).toBe(CODE);
  });

  it("rejects a code glued to letters, digits or hyphens", () => {
    expect(canonicalRfiInText(`X${CODE}`)).toBeNull();
    expect(canonicalRfiInText(`${CODE}9`)).toBeNull();
    expect(canonicalRfiInText(`${CODE}-2`)).toBeNull();
  });
});

describe("summarizeWorkbookCellChanges", () => {
  it("returns nothing for a plan without workbook updates", () => {
    expect(summarizeWorkbookCellChanges(planWith([]))).toEqual([]);
  });

  it("maps sync updates to Excel addresses, including multi-letter columns", () => {
    const changes = summarizeWorkbookCellChanges(
      planWith(
        [
          syncStep({
            Log: {
              a: { row: 1, col: 1, expectedValue: "old", replacementValue: "new", rowId: "r1" },
              b: { row: 3, col: 27, expectedValue: "", replacementValue: "x", rowId: "r1" },
            },
          }),
        ],
        { editor: { rows: [{ _rowId: "r1", canonicalRfi: CODE }] } },
      ),
    );
    expect(changes).toEqual([
      { workbook: "rfiLog", sheet: "Log", cell: "A1", before: "old", after: "new", rfiCode: CODE },
      { workbook: "rfiLog", sheet: "Log", cell: "AA3", before: "", after: "x", rfiCode: CODE },
    ]);
  });

  it("skips sync updates with an out-of-range row or missing row id", () => {
    const changes = summarizeWorkbookCellChanges(
      planWith([
        syncStep({
          Log: {
            bad: { row: 0, col: 1, expectedValue: "a", replacementValue: "b", rowId: "r1" },
            noId: { row: 2, col: 1, expectedValue: "a", replacementValue: "b" },
          },
        }),
      ]),
    );
    expect(changes).toEqual([]);
  });

  it("emits a follow-up address change and a status change from legacy updates", () => {
    const changes = summarizeWorkbookCellChanges(
      planWith([
        {
          kind: "workbook_update",
          meta: {
            updates: [
              {
                target: {
                  worksheet: { name: "FollowUp" },
                  row: 5,
                  statusColumn: 4,
                  rfiColumn: 2,
                  statusCell: { value: "Open" },
                  rfiCell: { value: "OLD" },
                },
                proposedStatus: "Closed",
                proposedRfi: "NEW",
                canonicalRfi: CODE,
              },
            ],
          },
        } as unknown as Step,
      ]),
    );
    expect(changes).toEqual([
      { workbook: "followUp", sheet: "FollowUp", cell: "B5", before: "OLD", after: "NEW", rfiCode: CODE },
      { workbook: "followUp", sheet: "FollowUp", cell: "D5", before: "Open", after: "Closed", rfiCode: CODE },
    ]);
  });

  it("drops a non-canonical candidate code instead of showing it", () => {
    const changes = summarizeWorkbookCellChanges(
      planWith([
        {
          kind: "workbook_update",
          meta: {
            updates: [
              {
                target: {
                  worksheet: { name: "FollowUp" },
                  row: 1,
                  statusColumn: 1,
                  statusCell: { value: "a" },
                },
                proposedStatus: "b",
                canonicalRfi: "lowercase-code",
              },
            ],
          },
        } as unknown as Step,
      ]),
    );
    expect(changes).toHaveLength(1);
    expect(changes[0].rfiCode).toBe("");
  });

  it("stops at the preview cap", () => {
    const updates: Record<string, unknown> = {};
    for (let i = 1; i <= MAX_WORKBOOK_CELL_PREVIEWS + 5; i += 1) {
      updates[`u${i}`] = { row: i, col: 1, expectedValue: "a", replacementValue: "b", rowId: "r" };
    }
    expect(summarizeWorkbookCellChanges(planWith([syncStep({ Log: updates })]))).toHaveLength(
      MAX_WORKBOOK_CELL_PREVIEWS,
    );
  });
});
