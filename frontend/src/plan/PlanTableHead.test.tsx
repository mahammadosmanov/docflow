import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { PlanTableHead } from "./PlanTableHead";

const s = makeStrings("en");
const columns = [
  { key: "owner", labelKey: "owner.label" },
  { key: "note", labelKey: "note.label" },
];

afterEach(cleanup);

function renderHead(overrides: Partial<Parameters<typeof PlanTableHead>[0]> = {}) {
  const props = {
    showSelection: true,
    showActions: false,
    allSelected: false,
    onSelectAll: vi.fn(),
    onSelectNone: vi.fn(),
    visibleColumns: columns,
    columnWidth: () => 120,
    headerId: "plan-head",
    onColumnWidthChange: vi.fn(),
    s,
    ...overrides,
  };
  render(
    <table>
      <PlanTableHead {...props} />
    </table>,
  );
  return props;
}

describe("PlanTableHead", () => {
  it("calls onSelectAll when the header checkbox is ticked", () => {
    const props = renderHead();
    const checkbox = screen.getByRole("checkbox", { name: s.t("editor.selectAll") });
    fireEvent.click(checkbox);
    expect(props.onSelectAll).toHaveBeenCalledTimes(1);
  });

  it("shows the checkbox state it is given", () => {
    renderHead({ allSelected: true });
    const checkbox = screen.getByRole("checkbox", { name: s.t("editor.selectAll") }) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });

  it("omits the selection and action columns when they are disabled", () => {
    renderHead({ showSelection: false, showActions: false });
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(document.querySelector(".editor-actions-header")).toBeNull();
  });

  it("adds the action column when row actions are enabled", () => {
    renderHead({ showActions: true });
    expect(document.querySelector(".editor-actions-header")).not.toBeNull();
  });

  it("renders one header per visible column", () => {
    renderHead({ showSelection: false });
    expect(document.querySelectorAll("th.adjustable-column-header")).toHaveLength(2);
  });
});
