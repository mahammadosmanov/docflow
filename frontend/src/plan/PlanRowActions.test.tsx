import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { PlanRowActions } from "./PlanRowActions";

const s = makeStrings("en");

afterEach(cleanup);

function renderActions(overrides: Partial<Parameters<typeof PlanRowActions>[0]> = {}) {
  const props = {
    rowIndex: 1,
    rowCount: 3,
    groupCount: 2,
    reorderable: true,
    duplicateEnabled: true,
    onMove: vi.fn(),
    onDuplicate: vi.fn(),
    onRemove: vi.fn(),
    s,
    ...overrides,
  };
  render(
    <table>
      <tbody>
        <tr>
          <PlanRowActions {...props} />
        </tr>
      </tbody>
    </table>,
  );
  return props;
}

describe("PlanRowActions", () => {
  it("moves the row up and down through the reorder buttons", () => {
    const props = renderActions();
    fireEvent.click(screen.getByRole("button", { name: s.t("action.moveUp") }));
    fireEvent.click(screen.getByRole("button", { name: s.t("action.moveDown") }));
    expect(props.onMove).toHaveBeenNthCalledWith(1, 1, -1);
    expect(props.onMove).toHaveBeenNthCalledWith(2, 1, 1);
  });

  it("disables move up on the first row and move down on the last", () => {
    renderActions({ rowIndex: 0, rowCount: 1 });
    expect((screen.getByRole("button", { name: s.t("action.moveUp") }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: s.t("action.moveDown") }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("duplicates the row by its index", () => {
    const props = renderActions();
    fireEvent.click(screen.getByTitle(s.t("rfi.action.duplicate")));
    expect(props.onDuplicate).toHaveBeenCalledWith(1);
  });

  it("refuses to remove the only row of a group", () => {
    const props = renderActions({ groupCount: 1 });
    const remove = screen.getByTitle(s.t("rfi.action.removeDuplicate")) as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
    fireEvent.click(remove);
    expect(props.onRemove).not.toHaveBeenCalled();
  });

  it("omits the buttons for features that are not enabled", () => {
    renderActions({ reorderable: false, duplicateEnabled: false });
    expect(screen.queryByRole("button")).toBeNull();
  });
});
