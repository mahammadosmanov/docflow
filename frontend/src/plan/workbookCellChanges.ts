import { type Plan } from "../types";

export function canonicalRfiInText(value: string): string | null {
  return (
    value.match(
      /(?:^|[^A-Z0-9-])(AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4})(?=$|[^A-Z0-9-])/,
    )?.[1] ?? null
  );
}

export type WorkbookCellChange = {
  workbook: string;
  sheet: string;
  cell: string;
  before: string;
  after: string;
  rfiCode: string;
};

export const MAX_WORKBOOK_CELL_PREVIEWS = 1_000;

function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function previewText(value: unknown): string | null {
  return typeof value === "string" && value.length <= 500 ? value : null;
}

function excelCell(row: unknown, column: unknown): string | null {
  if (
    typeof row !== "number" ||
    typeof column !== "number" ||
    !Number.isInteger(row) ||
    !Number.isInteger(column) ||
    row < 1 ||
    row > 1_048_576 ||
    column < 1 ||
    column > 16_384
  ) {
    return null;
  }
  let value = column;
  let letters = "";
  while (value > 0) {
    value -= 1;
    letters = String.fromCharCode(65 + (value % 26)) + letters;
    value = Math.floor(value / 26);
  }
  return `${letters}${row}`;
}

export function summarizeWorkbookCellChanges(plan: Plan): WorkbookCellChange[] {
  const result: WorkbookCellChange[] = [];
  const rowCodes = new Map<string, string>();
  const editor = recordValue(plan.meta?.editor);
  const editorRows = Array.isArray(editor?.rows) ? editor.rows : [];
  for (const rawRow of editorRows.slice(0, 2_000)) {
    const row = recordValue(rawRow);
    const rowId = previewText(row?._rowId);
    const code = previewText(row?.canonicalRfi) ?? previewText(row?.rfiNo);
    if (rowId && code && canonicalRfiInText(code) === code)
      rowCodes.set(rowId, code);
  }
  const add = (change: WorkbookCellChange) => {
    if (result.length < MAX_WORKBOOK_CELL_PREVIEWS) result.push(change);
  };
  for (const step of plan.steps) {
    if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    if (step.kind !== "workbook_update") continue;
    const sync = recordValue(step.meta?.syncStep);
    const syncUpdates = recordValue(sync?.updates);
    if (sync && syncUpdates) {
      const workbook =
        sync.workbookKind === "rfiLog" || sync.workbookKind === "followUp"
          ? sync.workbookKind
          : "workbook";
      for (const [sheetName, rawSheet] of Object.entries(syncUpdates)) {
        const sheet = previewText(sheetName);
        const sheetUpdates = recordValue(rawSheet);
        if (!sheet || !sheetUpdates) continue;
        for (const rawUpdate of Object.values(sheetUpdates)) {
          const update = recordValue(rawUpdate);
          const cell = excelCell(update?.row, update?.col);
          const before = previewText(update?.expectedValue);
          const after = previewText(update?.replacementValue);
          const rowId = previewText(update?.rowId);
          if (!cell || before === null || after === null || !rowId) continue;
          add({
            workbook,
            sheet,
            cell,
            before,
            after,
            rfiCode: rowCodes.get(rowId) ?? "",
          });
          if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
        }
      }
      continue;
    }
    const updates = Array.isArray(step.meta?.updates) ? step.meta.updates : [];
    for (const rawUpdate of updates) {
      const update = recordValue(rawUpdate);
      const target = recordValue(update?.target);
      const worksheet = recordValue(target?.worksheet);
      const statusCell = recordValue(target?.statusCell);
      const rfiCell = recordValue(target?.rfiCell);
      const sheet = previewText(worksheet?.name);
      const cell = excelCell(target?.row, target?.statusColumn);
      const before = previewText(statusCell?.value);
      const after = previewText(update?.proposedStatus);
      const candidateCode = previewText(update?.canonicalRfi) ?? "";
      const rfiCode =
        canonicalRfiInText(candidateCode) === candidateCode
          ? candidateCode
          : "";
      const rfiAddress = excelCell(target?.row, target?.rfiColumn);
      const rfiBefore = previewText(rfiCell?.value);
      const rfiAfter = previewText(update?.proposedRfi);
      if (
        sheet &&
        rfiAddress &&
        rfiBefore !== null &&
        rfiAfter !== null &&
        rfiBefore !== rfiAfter
      ) {
        add({
          workbook: "followUp",
          sheet,
          cell: rfiAddress,
          before: rfiBefore,
          after: rfiAfter,
          rfiCode,
        });
        if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
      }
      if (!sheet || !cell || before === null || after === null) continue;
      add({ workbook: "followUp", sheet, cell, before, after, rfiCode });
      if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    }
  }
  return result;
}
