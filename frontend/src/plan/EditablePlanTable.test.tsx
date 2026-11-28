import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { EditablePlanTable } from "./EditablePlanTable";

const s = makeStrings("en");

afterEach(cleanup);

describe("EditablePlanTable group selection", () => {
  it("reads group values linearly and leaves row selection on the checkboxes", () => {
    let groupReads = 0;
    const rows = Array.from({ length: 80 }, (_, index) => ({
      _rowId: String(index),
      get rfiNo() {
        groupReads += 1;
        return `RFI-${Math.floor(index / 2)}`;
      },
    }));
    render(
      <EditablePlanTable
        editor={{ kind: "recordTable", columns: [], groupField: "rfiNo", selectionEnabled: true }}
        rows={rows}
        s={s}
        onChange={vi.fn()}
      />,
    );
    expect(groupReads).toBeLessThan(rows.length * 10);
    expect(screen.getAllByRole("row").every((row) => !row.hasAttribute("tabindex"))).toBe(true);
    expect(screen.getAllByRole("checkbox")).toHaveLength(rows.length + 1);
  });

  it("names the group button by its action, not by the row count", () => {
    const rows = [
      { rfiNo: "RFI-7", enabled: true },
      { rfiNo: "RFI-7", enabled: true },
      { rfiNo: "RFI-9", enabled: true },
    ];
    render(
      <EditablePlanTable
        editor={{
          kind: "recordTable",
          columns: [],
          groupField: "rfiNo",
          selectionEnabled: true,
        }}
        rows={rows}
        s={s}
        onChange={vi.fn()}
      />,
    );
    const [group] = screen.getAllByRole("button", { name: s.t("editor.selectGroup") });
    expect(group.textContent).toBe("2");
    fireEvent.click(group);
    expect(screen.getAllByRole("checkbox", { checked: true })).toHaveLength(2);
  });
});
