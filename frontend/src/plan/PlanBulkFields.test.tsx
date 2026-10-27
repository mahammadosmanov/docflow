import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { PlanBulkFields } from "./PlanBulkFields";

const s = makeStrings("en");
const textColumn = { key: "owner", labelKey: "owner.label", kind: "text" };

afterEach(cleanup);

function renderFields(overrides: Partial<Parameters<typeof PlanBulkFields>[0]> = {}) {
  const props = {
    bulkField: "owner",
    bulkValue: "",
    bulkColumns: [textColumn],
    bulkColumn: textColumn,
    bulkChoiceValues: [],
    bulkSuggestions: [],
    bulkListId: "bulk-owner",
    choiceLabel: (_field: string, value: string) => value,
    canApply: true,
    onFieldChange: vi.fn(),
    onValueChange: vi.fn(),
    onApply: vi.fn(),
    s,
    ...overrides,
  };
  render(<PlanBulkFields {...props} />);
  return props;
}

describe("PlanBulkFields", () => {
  it("reports a new bulk field chosen from the field list", () => {
    const props = renderFields({ bulkColumns: [textColumn, { key: "note", labelKey: "note.label" }] });
    fireEvent.change(screen.getByLabelText(s.t("editor.bulk.field")), { target: { value: "note" } });
    expect(props.onFieldChange).toHaveBeenCalledWith("note");
  });

  it("reports text typed into the value box", () => {
    const props = renderFields();
    fireEvent.change(screen.getByLabelText(s.t("editor.bulk.value")), { target: { value: "Ali" } });
    expect(props.onValueChange).toHaveBeenCalledWith("Ali");
  });

  it("applies only when the bulk edit is complete", () => {
    const enabled = renderFields();
    fireEvent.click(screen.getByRole("button", { name: s.t("editor.bulk.apply") }));
    expect(enabled.onApply).toHaveBeenCalledTimes(1);
    cleanup();
    const disabled = renderFields({ canApply: false });
    const button = screen.getByRole("button", { name: s.t("editor.bulk.apply") }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(disabled.onApply).not.toHaveBeenCalled();
  });
});
