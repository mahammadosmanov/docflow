import { describe, expect, it } from "vitest";

import type { Plan } from "../types";
import { hasNewWarnings } from "./revisionWarnings";

const planWith = (warnings: { key: string; subject: string }[]) =>
  ({ warnings }) as unknown as Plan;

describe("hasNewWarnings", () => {
  it("is false when the revision adds no warning", () => {
    const before = planWith([{ key: "k", subject: "a" }]);
    expect(hasNewWarnings(before, planWith([{ key: "k", subject: "a" }]))).toBe(false);
    expect(hasNewWarnings(before, planWith([]))).toBe(false);
  });

  it("is true when a warning key or subject is new", () => {
    const before = planWith([{ key: "k", subject: "a" }]);
    expect(hasNewWarnings(before, planWith([{ key: "other", subject: "a" }]))).toBe(true);
    expect(hasNewWarnings(before, planWith([{ key: "k", subject: "b" }]))).toBe(true);
  });

  it("is false when the previous plan had no warnings and the revision has none", () => {
    expect(hasNewWarnings(planWith([]), planWith([]))).toBe(false);
  });
});
