import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { PlanFilterBar } from "./PlanFilterBar";

const s = makeStrings("en");

afterEach(cleanup);

function renderBar(overrides: Partial<Parameters<typeof PlanFilterBar>[0]> = {}) {
  const props = {
    searchTerm: "",
    filterMode: "all" as const,
    onSearchTermChange: vi.fn(),
    onFilterModeChange: vi.fn(),
    filteredCount: 3,
    totalCount: 3,
    s,
    ...overrides,
  };
  render(<PlanFilterBar {...props} />);
  return props;
}

describe("PlanFilterBar", () => {
  it("reports typed search text", () => {
    const props = renderBar();
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "RFI-1" } });
    expect(props.onSearchTermChange).toHaveBeenCalledWith("RFI-1");
  });

  it("reports the chosen filter tab", () => {
    const props = renderBar();
    fireEvent.click(screen.getByRole("button", { name: s.t("filter.changes") }));
    expect(props.onFilterModeChange).toHaveBeenCalledWith("changes");
  });

  it("shows the filtered count only while a search or filter is active", () => {
    const { rerender } = render(
      <PlanFilterBar
        searchTerm=""
        filterMode="all"
        onSearchTermChange={vi.fn()}
        onFilterModeChange={vi.fn()}
        filteredCount={2}
        totalCount={3}
        s={s}
      />,
    );
    expect(screen.queryByText(s.t("table.filterCount", { filtered: 2, total: 3 }))).toBeNull();
    rerender(
      <PlanFilterBar
        searchTerm="x"
        filterMode="all"
        onSearchTermChange={vi.fn()}
        onFilterModeChange={vi.fn()}
        filteredCount={2}
        totalCount={3}
        s={s}
      />,
    );
    expect(screen.getByText(s.t("table.filterCount", { filtered: 2, total: 3 }))).not.toBeNull();
  });
});
