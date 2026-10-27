import { describe, expect, it } from "vitest";

import type { Plan, Step } from "../types";
import { summarizePlanImpact } from "./planImpact";

const planWith = (steps: Array<Pick<Step, "kind">>, warnings: unknown[] = []) =>
  ({ planId: "p", opId: "op", steps, warnings, meta: {} }) as unknown as Plan;

const step = (kind: string) => ({ kind }) as Pick<Step, "kind">;

describe("summarizePlanImpact", () => {
  it("returns all zeros for an empty plan", () => {
    expect(summarizePlanImpact(planWith([]))).toEqual({
      outputs: 0,
      updates: 0,
      moves: 0,
      printJobs: 0,
      drafts: 0,
      cleanup: 0,
      warnings: 0,
    });
  });

  it("groups each step kind into its impact bucket", () => {
    const impact = summarizePlanImpact(
      planWith([
        step("copy"),
        step("write"),
        step("pdf_page_rotate"),
        step("workbook_update"),
        step("restore_backup"),
        step("move"),
        step("rename"),
        step("print"),
        step("outlook_reply_draft"),
        step("cleanup_temp"),
      ]),
    );
    expect(impact).toMatchObject({
      outputs: 3,
      updates: 2,
      moves: 2,
      printJobs: 1,
      drafts: 1,
      cleanup: 1,
    });
  });

  it("does not count directory creation toward any bucket", () => {
    const impact = summarizePlanImpact(planWith([step("mkdir")]));
    expect(impact).toMatchObject({
      outputs: 0,
      updates: 0,
      moves: 0,
      printJobs: 0,
      drafts: 0,
      cleanup: 0,
    });
  });

  it("reports the warning count separately from the steps", () => {
    const impact = summarizePlanImpact(planWith([step("copy")], [{}, {}]));
    expect(impact.warnings).toBe(2);
    expect(impact.outputs).toBe(1);
  });
});
