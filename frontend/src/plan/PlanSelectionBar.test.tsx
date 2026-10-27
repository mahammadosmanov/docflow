import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { PlanSelectionBar } from "./PlanSelectionBar";

const s = makeStrings("en");

afterEach(cleanup);

function renderBar() {
  const props = {
    selectedCount: 2,
    onSelectAll: vi.fn(),
    onSelectNone: vi.fn(),
    groupChoices: [{ field: "rfiNo", value: "RFI-7" }],
    compact: false,
    groupField: "rfiNo",
    onSelectGroup: vi.fn(),
    s,
  };
  render(<PlanSelectionBar {...props} />);
  return props;
}

describe("PlanSelectionBar", () => {
  it("shows the selected count", () => {
    renderBar();
    expect(screen.getByText(s.t("editor.selectedCount", { count: 2 }))).not.toBeNull();
  });

  it("selects all and selects none through their callbacks", () => {
    const props = renderBar();
    fireEvent.click(screen.getByRole("button", { name: s.t("editor.selectAll") }));
    fireEvent.click(screen.getByRole("button", { name: s.t("editor.selectNone") }));
    expect(props.onSelectAll).toHaveBeenCalledTimes(1);
    expect(props.onSelectNone).toHaveBeenCalledTimes(1);
  });

  it("selects a group by its field and value", () => {
    const props = renderBar();
    fireEvent.click(screen.getByRole("button", { name: "RFI-7" }));
    expect(props.onSelectGroup).toHaveBeenCalledWith("rfiNo", "RFI-7");
  });
});
