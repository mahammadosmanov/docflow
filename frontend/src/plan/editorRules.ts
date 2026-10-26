import { type Plan } from "../types";

export function copySheetsFromPlan(plan: Plan | null): string[] {
  const editor = plan?.meta?.editor;
  if (!editor || typeof editor !== "object") return [];
  const sources = (editor as Record<string, unknown>).copySources;
  return sources && typeof sources === "object"
    ? Object.keys(sources as Record<string, unknown>)
    : [];
}

export type EditorColumn = {
  key: string;
  labelKey: string;
  kind?: string;
  width?: number;
  sheets?: string[];
  detail?: boolean;
  bulk?: boolean;
  display?: string;
  disabledWhen?: { field: string; equals: string | number | boolean };
};

export function displayEditorValue(column: EditorColumn, value: string): string {
  if (column.display === "suffix") {
    return value.match(/(\d+)\s*$/)?.[1] ?? value;
  }
  if (column.display === "shortNumber") {
    const suffix = value.match(/(\d+)\s*$/)?.[1];
    return suffix ? String(Number(suffix)) : value;
  }
  if (column.display === "itemSuffix") {
    return value.replace(/^[A-Z]{2,5}[-\u02d7\u2010-\u2015]+/i, "");
  }
  if (column.display === "percent") {
    return formatConfidencePercent(value, "fraction") ?? "—";
  }
  return value;
}

export function formatConfidencePercent(
  value: unknown,
  scale: "fraction" | "percent" = "fraction",
): string | null {
  const numeric = typeof value === "number" ? value : Number(value);
  if (
    (typeof value === "string" && value.trim() === "") ||
    !Number.isFinite(numeric)
  ) {
    return null;
  }
  const percent = scale === "fraction" ? numeric * 100 : numeric;
  return `${Math.round(Math.min(100, Math.max(0, percent)))}%`;
}

export function editorColumnDefaultWidth(column: EditorColumn) {
  return column.width ??
    (column.kind === "bool"
      ? 50
      : column.key === "sourceStatuses" || column.key === "disposition"
        ? 220
        : column.key === "reasonKey"
          ? 180
          : column.key === "currentStatus" || column.key === "proposedStatus"
            ? 130
            : column.key === "rfiNo"
              ? 120
              : column.key === "_sheet"
                ? 60
                : column.key === "_row"
                  ? 50
                  : 180);
}
export function editorColumnMinWidth(column: EditorColumn) {
  return column.kind === "compactDate" ? 136 : 48;
}

function isCanonicalRfiCode(value: unknown): value is string {
  return (
    typeof value === "string" &&
    /^AZE-SUN-INT-QMS-RFI-(?:CIV|MEC|ELE|PRC)-\d{4}$/.test(value)
  );
}

export function isRfiCodeCandidate(value: unknown): value is string {
  return (
    isCanonicalRfiCode(value) ||
    (typeof value === "string" &&
      /^AZE-SUN-INT-QMS-RFI-(?:CIV|MEC|ELE|PRC)-[0-9]{1,3}(?![\s\S])/.test(
        value,
      ))
  );
}

export function isPlanEditorReady(
  editor: Record<string, unknown>,
  rows: Record<string, unknown>[],
): boolean {
  const requiredFields = Array.isArray(editor.requiredFields)
    ? editor.requiredFields.filter(
        (field): field is string => typeof field === "string",
      )
    : [];
  const requiredValuesReady = rows.every(
    (row) =>
      row.enabled === false ||
      requiredFields.every((field) => {
        const value = row[field];
        return typeof value === "string"
          ? value.trim().length > 0
          : value !== null && value !== undefined;
      }),
  );
  if (!requiredValuesReady) return false;
  if (editor.kind !== "rfiScan") return true;
  const codes = rows.map((row) => row.code);
  return (
    codes.every(isRfiCodeCandidate) && new Set(codes).size === codes.length
  );
}

/** Writes one cell, then applies the editor's auto-fill and regex-fill presets. */
export function applyEditorValue(
  editor: Record<string, unknown>,
  row: Record<string, unknown>,
  key: string,
  value: string | boolean,
) {
  row[key] = value;

  const autoFill = editor.autoFill;
  if (autoFill && typeof autoFill === "object") {
    const config = autoFill as Record<string, unknown>;
    if (
      config.field === key &&
      config.values &&
      typeof config.values === "object"
    ) {
      const preset = (config.values as Record<string, unknown>)[
        String(value)
      ];
      if (preset && typeof preset === "object") Object.assign(row, preset);
    }
  }

  if (Array.isArray(editor.regexFill)) {
    for (const rawRule of editor.regexFill) {
      if (!rawRule || typeof rawRule !== "object") continue;
      const rule = rawRule as Record<string, unknown>;
      if (rule.source !== key) continue;
      try {
        if (new RegExp(String(rule.pattern), "i").test(String(value))) {
          row[String(rule.target)] = String(rule.value ?? "");
        }
      } catch {
        // Invalid server-side presentation hints never block row editing.
      }
    }
  }
}

/** Copies a row after its group. Returns a new list; the source list is untouched. */
export function duplicateRowAt(
  rows: Record<string, unknown>[],
  index: number,
  groupField: string,
  stamp: number = Date.now(),
): Record<string, unknown>[] {
  const source = rows[index];
  const group = source[groupField];
  let insertAt = index + 1;
  while (insertAt < rows.length && rows[insertAt][groupField] === group) {
    insertAt += 1;
  }
  const clone = {
    ...source,
    _rowId: `${String(source._rowId ?? group)}-copy-${stamp}`,
  };
  const next = [...rows];
  next.splice(insertAt, 0, clone);
  return next;
}

/** Drops a row unless it is the last one in its group. Null means refused. */
export function removeRowAt(
  rows: Record<string, unknown>[],
  index: number,
  groupField: string,
): Record<string, unknown>[] | null {
  const group = rows[index][groupField];
  if (rows.filter((row) => row[groupField] === group).length <= 1) return null;
  return rows.filter((_, rowIndex) => rowIndex !== index);
}

/** Swaps a row with its neighbour. Null when the move would leave the list. */
export function moveRowAt(
  rows: Record<string, unknown>[],
  index: number,
  direction: -1 | 1,
): Record<string, unknown>[] | null {
  const target = index + direction;
  if (target < 0 || target >= rows.length) return null;
  const next = [...rows];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

export type PlanRowFilter = "all" | "changes" | "review";

/** Indices of rows that pass the filter. `normalizedSearch` is trimmed and lowercased by the caller. */
export function filterRowIndices(
  rows: Record<string, unknown>[],
  filterMode: PlanRowFilter,
  normalizedSearch: string,
): number[] {
  return rows.flatMap((row, index) => {
    if (filterMode === "changes") {
      const isChange =
        (row.proposedStatus && row.proposedStatus !== row.currentStatus) ||
        row.enabled === true;
      if (!isChange) return [];
    } else if (filterMode === "review") {
      const isReview =
        row.disposition === "manual_review" ||
        Boolean(row.reasonKey) ||
        row.selectable === false ||
        row.enabled === false;
      if (!isReview) return [];
    }
    if (normalizedSearch) {
      const matches = Object.values(row).some((val) =>
        typeof val === "string" || typeof val === "number"
          ? String(val).toLowerCase().includes(normalizedSearch)
          : false,
      );
      if (!matches) return [];
    }
    return [index];
  });
}
