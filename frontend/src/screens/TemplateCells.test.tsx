import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { TemplateStructure } from "../types";
import { TemplateCells, cellLocation } from "./TemplateCells";

const s = makeStrings("en");

const word: TemplateStructure = {
  kind: "docx",
  suggestions: [
    { key: "rfiNo", label: "RFI No.", value: "RFI No: {rfiNo}", anchor: { table: 0, row: 0, cell: 1 } },
  ],
  tables: [
    {
      truncated: false,
      rows: [
        {
          cells: [
            { text: "Label", colSpan: 1, problem: null },
            { text: "RFI No: AZE-SUN-INT-QMS-RFI-ELE-0049", colSpan: 1, problem: null },
            { text: "", colSpan: 1, problem: "verticalMerge" },
          ],
        },
      ],
    },
  ],
};

const excel: TemplateStructure = {
  kind: "xlsx",
  suggestions: [],
  sheets: [
    {
      name: "Form",
      maxRow: 2,
      maxColumn: 3,
      truncated: false,
      protected: false,
      cells: [
        { row: 1, column: 1, text: "Title", formula: false },
        { row: 2, column: 1, text: "2", formula: true },
      ],
      merges: [{ row: 1, column: 1, rowSpan: 1, colSpan: 2 }],
    },
  ],
};

function mount(
  structure: TemplateStructure,
  fields = [] as { key: string }[],
  copies: unknown = null,
) {
  const onMark = vi.fn();
  const bridge = {
    call: vi.fn((method: string) =>
      Promise.resolve(method === "templates/suggestFromCopies" ? copies : structure),
    ),
  } as unknown as Bridge;
  render(
    <TemplateCells
      bridge={bridge}
      s={s}
      versionId="v"
      fields={fields as never}
      reloadKey=""
      onMark={onMark}
    />,
  );
  return { onMark, bridge };
}

describe("TemplateCells", () => {
  afterEach(cleanup);

  it("describes where a cell sits", () => {
    expect(cellLocation(s, { sheet: "Form", address: "C3" })).toBe("Cell C3 on Form");
    expect(cellLocation(s, { table: 0, row: 1, cell: 2 })).toBe("Table 1, row 2, cell 3");
  });

  it("marks a Word cell with its table position and a label", async () => {
    const { onMark, bridge } = mount(word);
    fireEvent.click(await screen.findByRole("button", { name: "Label" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Pour number" } });
    fireEvent.click(screen.getByRole("button", { name: "Make editable" }));
    expect(bridge.call).toHaveBeenCalledWith("templates/cells", { versionId: "v" });
    expect(onMark).toHaveBeenCalledWith(
      expect.objectContaining({
        key: "pourNumber",
        label: "Pour number",
        value: "Label",
        anchor: { table: 0, row: 0, cell: 0 },
      }),
    );
  });

  it("shows why a Word cell is locked and does not select it", async () => {
    mount(word);
    const locked = await screen.findByTitle("Part of a vertically merged cell");
    fireEvent.click(locked);
    expect(screen.queryByRole("button", { name: "Make editable" })).toBeNull();
  });

  it("offers the RFI number first and keeps the cell's label in the start value", async () => {
    const { onMark } = mount(word);
    fireEvent.click(await screen.findByRole("button", { name: /Suggested: RFI number/ }));
    expect(onMark).toHaveBeenCalledWith(
      expect.objectContaining({ key: "rfiNo", value: "RFI No: {rfiNo}", anchor: { table: 0, row: 0, cell: 1 } }),
    );
  });

  it("hides the suggestion once that field exists", async () => {
    mount(word, [{ key: "rfiNo" }]);
    await screen.findByRole("button", { name: "Label" });
    expect(screen.queryByRole("button", { name: /Suggested/ })).toBeNull();
  });

  it("marks an Excel cell by address, skips merged cover cells and locks formulas", async () => {
    const { onMark } = mount(excel);
    const title = await screen.findByRole("button", { name: "Title" });
    expect(title.getAttribute("colspan")).toBe("2");
    expect(screen.getByTitle("Formula cell")).toBeTruthy();
    fireEvent.click(title);
    fireEvent.click(screen.getByRole("button", { name: "Make editable" }));
    await waitFor(() =>
      expect(onMark).toHaveBeenCalledWith(
        expect.objectContaining({ anchor: { sheet: "Form", address: "A1" } }),
      ),
    );
  });

  it("marks cells already chosen", async () => {
    mount(word, [{ key: "a", anchor: { table: 0, row: 0, cell: 0 } } as never]);
    const cell = await screen.findByRole("button", { name: "Label" });
    expect(cell.className).toContain("marked");
  });

  const copies = {
    suggestions: [
      { anchor: { table: 0, row: 0, cell: 1 }, label: "RFI", value: "RFI No: {rfiNo}", samples: ["0049", "0050"], rfi: true },
      { anchor: { table: 0, row: 0, cell: 0 }, label: "Pour", value: "P1", samples: ["P1", "P2", "P3"], rfi: false },
      { anchor: { table: 0, row: 0, cell: 0 }, label: "Pour", value: "", samples: ["Q"], rfi: false },
    ],
  };

  it("suggests cells that differ between copies and marks one with its label", async () => {
    const { onMark, bridge } = mount(word, [], copies);
    fireEvent.click(await screen.findByRole("button", { name: "Suggest from other copies" }));
    expect(await screen.findByText("0049 · 0050")).toBeTruthy();
    expect(bridge.call).toHaveBeenCalledWith("templates/suggestFromCopies", { versionId: "v" });
    fireEvent.click(screen.getAllByRole("button", { name: "Make editable" })[0]);
    expect(onMark).toHaveBeenCalledWith(
      expect.objectContaining({ key: "rfiNo", label: "RFI", value: "RFI No: {rfiNo}", anchor: { table: 0, row: 0, cell: 1 } }),
    );
  });

  it("marks all suggestions with unique keys", async () => {
    const { onMark } = mount(word, [], copies);
    fireEvent.click(await screen.findByRole("button", { name: "Suggest from other copies" }));
    fireEvent.click(await screen.findByRole("button", { name: "Make all editable" }));
    const keys = onMark.mock.calls.map(([field]) => field.key);
    expect(keys).toEqual(["rfiNo", "pour", "pour2"]);
  });

  it("says so when the copies do not differ, and ignores a cancelled pick", async () => {
    const none = mount(word, [], { suggestions: [] });
    fireEvent.click(await screen.findByRole("button", { name: "Suggest from other copies" }));
    expect(await screen.findByText("The copies do not differ in any cell you can mark.")).toBeTruthy();
    none.onMark.mockClear();
    cleanup();
    mount(word, [], null);
    fireEvent.click(await screen.findByRole("button", { name: "Suggest from other copies" }));
    await waitFor(() => expect(screen.queryByText("Cells that differ")).toBeNull());
    expect(screen.queryByText("The copies do not differ in any cell you can mark.")).toBeNull();
  });
});
