import { useState } from "react";
import { clampColumnWidth } from "../components/AdjustableDataRegion";
import type { EditorMetadata } from "../types";
import {
  type EditorColumn,
  editorColumnDefaultWidth,
  editorColumnMinWidth,
} from "./editorRules";

/** Per-editor column widths, keyed by editor title and column key. */
export function useEditorColumnWidths(editor: EditorMetadata) {
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>({});
  const widthScope = String(editor.titleKey ?? editor.kind ?? "plan-editor");
  const columnWidth = (column: EditorColumn) =>
    clampColumnWidth(
      columnWidths[`${widthScope}:${column.key}`] ??
        editorColumnDefaultWidth(column),
      editorColumnMinWidth(column),
    );
  const setColumnWidth = (columnKey: string, nextWidth: number) =>
    setColumnWidths((current) => ({
      ...current,
      [`${widthScope}:${columnKey}`]: nextWidth,
    }));
  return { columnWidth, setColumnWidth };
}
