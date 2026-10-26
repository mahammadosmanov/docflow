import { describe, expect, it } from "vitest";

import type { OperationDef, Plan, ProgressEvent } from "../types";
import { editorRowsOf, nextProgressTrail, rememberedParams } from "./operationState";

const event = (phase: string, done = 0): ProgressEvent => ({ phase, done, total: 0 });

describe("operation state helpers", () => {
  it("replaces the last entry when the phase repeats and appends otherwise", () => {
    const trail = nextProgressTrail([event("scan", 1)], event("scan", 2));
    expect(trail).toEqual([event("scan", 2)]);
    expect(nextProgressTrail(trail, event("print"))).toEqual([event("scan", 2), event("print")]);
  });

  it("keeps only the last eight progress entries", () => {
    const phases = Array.from({ length: 10 }, (_, index) => event(`p${index}`));
    const trail = phases.reduce<ProgressEvent[]>(nextProgressTrail, []);
    expect(trail.map((item) => item.phase)).toEqual(phases.slice(2).map((item) => item.phase));
  });

  it("reads editor rows only when the plan carries them", () => {
    const rows = [{ code: "A" }];
    expect(editorRowsOf({ meta: { editor: { rows } } } as unknown as Plan)).toEqual(rows);
    expect(editorRowsOf({ meta: { editor: { rows: "no" } } } as unknown as Plan)).toEqual([]);
    expect(editorRowsOf({} as Plan)).toEqual([]);
  });

  it("keeps saved values that match a schema parameter and are not opted out", () => {
    const operation = {
      schema: [
        { name: "a", remember: true },
        { name: "b", remember: false },
        { name: "c" },
      ],
    } as unknown as OperationDef;
    expect(rememberedParams(operation, { a: 1, b: 2, c: 3, unknown: 4 })).toEqual({ a: 1, c: 3 });
  });
});
