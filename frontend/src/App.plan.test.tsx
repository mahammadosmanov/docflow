import { describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { useState } from "react";
import { makeStrings } from "./i18n";
import { PlanView } from "./plan/PlanView";
import type { Plan } from "./types";
import { AdjustableDataRegion, ColumnResizeHandle } from "./components/AdjustableDataRegion";
import { registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("adjustable plan data regions", () => {
  it("resizes columns with keyboard and pointer input within accessible bounds", () => {
    const pointerCaptureDescriptor = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "setPointerCapture",
    );
    Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
      configurable: true,
      value: vi.fn(),
    });
    const setWidth = vi.fn();
    render(
      <ColumnResizeHandle
        width={120}
        label="RFI number"
        s={makeStrings("en")}
        onWidthChange={setWidth}
      />,
    );
    const separator = screen.getByRole("separator", {
      name: "Resize RFI number column",
    });
    fireEvent.keyDown(separator, { key: "ArrowRight" });
    expect(setWidth).toHaveBeenLastCalledWith(136);
    fireEvent.keyDown(separator, { key: "Home" });
    expect(setWidth).toHaveBeenLastCalledWith(48);
    fireEvent.pointerDown(separator, {
      button: 0,
      pointerId: 1,
      clientX: 100,
    });
    fireEvent.pointerMove(separator, { pointerId: 1, clientX: 200 });
    expect(separator).toHaveAttribute("aria-valuenow", "220");
    fireEvent.pointerUp(separator, { pointerId: 1, clientX: 200 });
    expect(setWidth).toHaveBeenLastCalledWith(220);
    fireEvent.pointerDown(separator, {
      button: 0,
      pointerId: 2,
      clientX: 100,
    });
    fireEvent.pointerMove(separator, { pointerId: 2, clientX: 180 });
    fireEvent.pointerCancel(separator, { pointerId: 2 });
    expect(separator).toHaveAttribute("aria-valuenow", "120");
    expect(setWidth).toHaveBeenLastCalledWith(220);
    if (pointerCaptureDescriptor) {
      Object.defineProperty(
        HTMLElement.prototype,
        "setPointerCapture",
        pointerCaptureDescriptor,
      );
    } else {
      delete (HTMLElement.prototype as Partial<HTMLElement>).setPointerCapture;
    }
  });

  it("keeps a usable height minimum and clamps height when the window shrinks", () => {
    const originalHeight = window.innerHeight;
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 700,
    });
    render(
      <AdjustableDataRegion
        s={makeStrings("en")}
        className="test-data"
        label="Results"
      >
        <div>Rows</div>
      </AdjustableDataRegion>,
    );
    let separator = screen.getByRole("separator", {
      name: "Resize Results height",
    });
    fireEvent.keyDown(separator, { key: "Home" });
    fireEvent.keyDown(separator, { key: "ArrowDown" });
    expect(separator).toHaveAttribute("aria-valuenow", "156");
    fireEvent.keyDown(separator, { key: "ArrowUp" });
    expect(separator).toHaveAttribute("aria-valuenow", "140");
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 300,
    });
    fireEvent(window, new Event("resize"));
    separator = screen.getByRole("separator", {
      name: "Resize Results height",
    });
    expect(separator).toHaveAttribute("aria-valuemin", "140");
    expect(separator).toHaveAttribute("aria-valuenow", "140");
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: originalHeight,
    });
    fireEvent(window, new Event("resize"));
  });
  it("retains edited and selected rows and column width across filtering, detail toggles, and revision", () => {
    const initialRows = [
      { _rowId: "row-1", rfiNo: "MEC-0700", discipline: "MEC" },
    ];
    const basePlan: Plan = {
      planId: "adjustment-plan-0",
      opId: "rfi.system_sync",
      steps: [],
      warnings: [],
      meta: {
        editor: {
          kind: "recordTable",
          titleKey: "rfi.systemSync.editor.title",
          requiredFields: ["rfiNo"],
          selectionEnabled: true,
          detailsEnabled: true,
          detailsDefaultVisible: true,
          columns: [
            {
              key: "rfiNo",
              labelKey: "rfi.systemSync.column.rfiNo",
              kind: "text",
            },
            {
              key: "discipline",
              labelKey: "rfi.systemSync.column.discipline",
              kind: "text",
              detail: true,
            },
          ],
        },
      },
    };
    const StatefulPlanView = ({ revision }: { revision: number }) => {
      const [rows, setRows] = useState<Record<string, unknown>[]>(initialRows);
      const editor = {
        ...(basePlan.meta?.editor as Record<string, unknown>),
        rows,
      };
      const plan = {
        ...basePlan,
        planId: `adjustment-plan-${revision}`,
        meta: { editor },
      };
      return (
        <PlanView
          plan={plan}
          s={makeStrings("en")}
          error=""
          onCancel={vi.fn()}
          onApply={vi.fn()}
          editorRows={rows}
          onEditorRowsChange={setRows}
          editorDirty
          onUpdatePreview={vi.fn()}
        />
      );
    };

    const view = render(<StatefulPlanView revision={0} />);
    const rfiNo = screen.getByRole("textbox", { name: "RFI No." });
    fireEvent.change(rfiNo, { target: { value: "MEC-0701" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Select row 1" }));
    let resize = screen.getByRole("separator", {
      name: "Resize RFI No. column",
    });
    fireEvent.keyDown(resize, { key: "ArrowRight" });
    expect(resize).toHaveAttribute("aria-valuenow", "136");
    fireEvent.click(screen.getByRole("button", { name: "Hide automatic fields" }));
    fireEvent.click(screen.getByRole("button", { name: "Show automatic fields" }));
    fireEvent.change(screen.getByRole("searchbox"), {
      target: { value: "not a matching RFI" },
    });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
    view.rerender(<StatefulPlanView revision={1} />);

    expect(screen.getByRole("textbox", { name: "RFI No." })).toHaveValue("MEC-0701");
    expect(screen.getByRole("checkbox", { name: "Select row 1" })).toBeChecked();
    resize = screen.getByRole("separator", {
      name: "Resize RFI No. column",
    });
    expect(resize).toHaveAttribute("aria-valuenow", "136");
  });
});
