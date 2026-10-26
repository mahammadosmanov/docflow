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
