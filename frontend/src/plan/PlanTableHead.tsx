import { ColumnResizeHandle } from "../components/AdjustableDataRegion";
import { type Strings } from "../i18n";
import { type EditorColumn, editorColumnMinWidth } from "./editorRules";

/** Header row of the plan table: select-all checkbox, resizable column headers, and the action column. */
export function PlanTableHead({
  showSelection,
  showActions,
  allSelected,
  onSelectAll,
  onSelectNone,
  visibleColumns,
  columnWidth,
  headerId,
  onColumnWidthChange,
  s,
}: {
  showSelection: boolean;
  showActions: boolean;
  allSelected: boolean;
  onSelectAll: () => void;
  onSelectNone: () => void;
  visibleColumns: EditorColumn[];
  columnWidth: (column: EditorColumn) => number;
  headerId: string;
  onColumnWidthChange: (columnKey: string, width: number) => void;
  s: Strings;
}) {
  return (
    <thead>
      <tr>
        {showSelection && (
          <th className="selection-column">
            <input
              aria-label={s.t("editor.selectAll")}
              type="checkbox"
              checked={allSelected}
              onChange={(event) =>
                event.target.checked ? onSelectAll() : onSelectNone()
              }
            />
          </th>
        )}
        {visibleColumns.map((column) => {
          const label = s.t(column.labelKey);
          const width = columnWidth(column);
          return (
            <th
              key={column.key}
              className="adjustable-column-header"
              scope="col"
              aria-labelledby={`${headerId}-${column.key}`}
              style={{ width }}
              title={label}
            >
              <span id={`${headerId}-${column.key}`}>{label}</span>
              <ColumnResizeHandle
                width={width}
                label={label}
                minWidth={editorColumnMinWidth(column)}
                s={s}
                onWidthChange={(nextWidth) =>
                  onColumnWidthChange(column.key, nextWidth)
                }
              />
            </th>
          );
        })}
        {showActions && (
          <th
            className="editor-actions-header"
            aria-label={s.t("editor.rowActions")}
          />
        )}
      </tr>
    </thead>
  );
}
