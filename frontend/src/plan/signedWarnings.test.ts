import { describe, expect, it } from "vitest";

import type { Plan } from "../types";
import { signedWarningReviewItems } from "./signedWarnings";

const planWith = (warningReviewItems: unknown) =>
  ({ meta: { warningReviewItems } }) as unknown as Plan;

const item = (overrides: Record<string, unknown> = {}) => ({
  sourceFile: "a.pdf",
  destination: "b.pdf",
  status: "planned",
  reasons: [],
  ...overrides,
});

describe("signedWarningReviewItems", () => {
  it("returns nothing when the plan carries no review items", () => {
    expect(signedWarningReviewItems({} as Plan)).toEqual([]);
    expect(signedWarningReviewItems(planWith("not-a-list"))).toEqual([]);
  });

  it("keeps items whose status is one of the three known values", () => {
    const items = signedWarningReviewItems(
      planWith([
        item({ status: "planned" }),
        item({ status: "exists" }),
        item({ status: "blocked" }),
        item({ status: "unknown" }),
      ]),
    );
    expect(items.map((entry) => entry.status)).toEqual(["planned", "exists", "blocked"]);
  });

  it("drops items missing a file name or with a malformed reason list", () => {
    expect(signedWarningReviewItems(planWith([item({ destination: 7 })]))).toEqual([]);
    expect(signedWarningReviewItems(planWith([item({ reasons: "x" })]))).toEqual([]);
  });

  it("drops reasons without a key or detail and keeps the rest", () => {
    const [entry] = signedWarningReviewItems(
      planWith([
        item({
          reasons: [
            { key: "k1", detail: "d1" },
            { key: "k2" },
            null,
          ],
        }),
      ]),
    );
    expect(entry.reasons).toEqual([{ key: "k1", pageNumber: null, code: "", detail: "d1" }]);
  });

  it("keeps only positive safe-integer page numbers and text codes", () => {
    const reasonFor = (pageNumber: unknown, code: unknown) => {
      const [entry] = signedWarningReviewItems(
        planWith([
          item({ reasons: [{ key: "k", detail: "d", pageNumber, code }] }),
        ]),
      );
      return entry.reasons[0];
    };
    expect(reasonFor(3, "C1").pageNumber).toBe(3);
    expect(reasonFor(0, "C1").pageNumber).toBeNull();
    expect(reasonFor(-1, "C1").pageNumber).toBeNull();
    expect(reasonFor(1.5, "C1").pageNumber).toBeNull();
    expect(reasonFor("2", "C1").pageNumber).toBeNull();
    expect(reasonFor(3, 9).code).toBe("");
    expect(reasonFor(3, "C1").code).toBe("C1");
  });
});
