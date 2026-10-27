import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { planRowId, usePlanSelection } from "./usePlanSelection";

afterEach(cleanup);

const rows = [
  { _rowId: "a", group: "CIV" },
  { _rowId: "b", group: "MEC" },
  { group: "CIV" },
];

function setup(
  current = rows,
  filteredRowIndices = [0, 1, 2],
  groupField = "group",
) {
  return renderHook(
    (props: { rows: typeof rows }) =>
      usePlanSelection({ rows: props.rows, groupField, filteredRowIndices }),
    { initialProps: { rows: current } },
  );
}

describe("planRowId", () => {
  it("uses the row id when present and a group-and-position fallback otherwise", () => {
    expect(planRowId({ _rowId: "a" }, 0, "group")).toBe("a");
    expect(planRowId({ group: "CIV" }, 2, "group")).toBe("CIV-2");
    expect(planRowId({}, 4, "group")).toBe("row-4");
  });
});

describe("usePlanSelection", () => {
  it("selects only the rows that pass the current filter", () => {
    const hook = setup(rows, [0, 2]);
    act(() => hook.result.current.selectAll());
    expect([...hook.result.current.selectedIds].sort()).toEqual(["CIV-2", "a"]);
  });

  it("toggles a single row on and off", () => {
    const hook = setup();
    act(() => hook.result.current.toggleRow("b"));
    expect(hook.result.current.selectedRows).toEqual([rows[1]]);
    act(() => hook.result.current.toggleRow("b"));
    expect(hook.result.current.selectedIds.size).toBe(0);
  });

  it("selects every row in a group by field value", () => {
    const hook = setup();
    act(() => hook.result.current.selectGroup("group", "CIV"));
    expect([...hook.result.current.selectedIds].sort()).toEqual(["CIV-2", "a"]);
  });

  it("drops selections for rows that disappear", () => {
    const hook = setup();
    act(() => hook.result.current.selectAll());
    hook.rerender({ rows: [rows[0]] });
    expect([...hook.result.current.selectedIds]).toEqual(["a"]);
  });
});
