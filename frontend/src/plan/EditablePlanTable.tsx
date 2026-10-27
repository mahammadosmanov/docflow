import {
  AdjustableDataRegion,
  clampColumnWidth,
} from "../components/AdjustableDataRegion";
import {
  Eye,
  EyeOff,
  TableProperties,
} from "lucide-react";
import {
  useEffect,
  useId,
  useMemo,
  useState,
} from "react";
import {
  Button,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type EditorMetadata,
} from "../types";
import {
  CompactDateInput,
} from "./CompactDateInput";
import { PlanFilterBar } from "./PlanFilterBar";
import { usePlanSelection } from "./usePlanSelection";
import { PlanSelectionBar } from "./PlanSelectionBar";
import { PlanBulkFields } from "./PlanBulkFields";
import { PlanTableHead } from "./PlanTableHead";
import { PlanRowActions } from "./PlanRowActions";
import {
  type EditorColumn,
  displayEditorValue,
  editorColumnDefaultWidth,
  applyEditorValue,
  choicesForRow,
  filterRowIndices,
  duplicateRowAt,
  moveRowAt,
  removeRowAt,
  editorColumnMinWidth,
  isRfiCodeCandidate,
} from "./editorRules";

export * from "./editorRules";

export function EditablePlanTable({
  editor,
  rows,
  s,
  onChange,
}: {
  editor: EditorMetadata;
  rows: Record<string, unknown>[];
  s: Strings;
  onChange: (rows: Record<string, unknown>[]) => void;
}) {
  const columns = Array.isArray(editor.columns)
    ? (editor.columns as EditorColumn[])
    : [];
  const headerId = useId();
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>({});
  const widthScope = String(editor.titleKey ?? editor.kind ?? "plan-editor");
  const columnWidth = (column: EditorColumn) =>
    clampColumnWidth(
      columnWidths[`${widthScope}:${column.key}`] ??
        editorColumnDefaultWidth(column),
      editorColumnMinWidth(column),
    );
  const groupField = String(editor.groupField ?? "rfiNo");
  const autocomplete =
    editor.autocomplete && typeof editor.autocomplete === "object"
      ? (editor.autocomplete as Record<string, unknown>)
      : {};
  const choiceLabels =
    editor.choiceLabels && typeof editor.choiceLabels === "object"
      ? (editor.choiceLabels as Record<string, unknown>)
      : {};
  const [detailsVisible, setDetailsVisible] = useState(
    editor.detailsDefaultVisible === true,
  );
  const bulkColumns = columns.filter(
    (column) =>
      column.bulk !== false &&
      column.kind !== "readonly" &&
      (!column.detail || detailsVisible),
  );
  const [bulkField, setBulkField] = useState(bulkColumns[0]?.key ?? "");
  const [bulkValue, setBulkValue] = useState("");
  const visibleColumns = columns.filter(
    (column) => !column.detail || detailsVisible,
  );
  const hasRowActions =
    editor.reorderable === true || editor.duplicateEnabled === true;
  const tableWidth =
    (editor.selectionEnabled === true ? 52 : 0) +
    visibleColumns.reduce((total, column) => total + columnWidth(column), 0) +
    (hasRowActions ? 132 : 0);

  const [searchTerm, setSearchTerm] = useState("");
  const [filterMode, setFilterMode] = useState<"all" | "changes" | "review">(
    "all",
  );

  const normalizedSearch = searchTerm.trim().toLowerCase();
  const filteredRowIndices = useMemo(
    () => filterRowIndices(rows, filterMode, normalizedSearch),
    [rows, filterMode, normalizedSearch],
  );
  const {
    rowId,
    selectedIds,
    selectedRows,
    selectAll,
    selectNone,
    toggleRow,
    selectGroup,
  } = usePlanSelection({ rows, groupField, filteredRowIndices });

  useEffect(() => {
    if (bulkColumns.some((column) => column.key === bulkField)) return;
    setBulkField(bulkColumns[0]?.key ?? "");
    setBulkValue("");
  }, [bulkField, detailsVisible, editor.columns]);

  const choiceLabel = (field: string, value: string, column?: EditorColumn) => {
    const fieldLabels = choiceLabels[field];
    if (fieldLabels && typeof fieldLabels === "object") {
      const key = (fieldLabels as Record<string, unknown>)[value];
      if (typeof key === "string") return s.t(key);
    }
    return column ? displayEditorValue(column, value) : value;
  };

  const choicesFor = (column: EditorColumn, row: Record<string, unknown>) =>
    choicesForRow(editor, column, row);

  const applyValue = (
    row: Record<string, unknown>,
    key: string,
    value: string | boolean,
  ) => applyEditorValue(editor, row, key, value);

  const update = (index: number, key: string, value: string | boolean) => {
    const next = rows.map((row) => ({ ...row }));
    applyValue(next[index], key, value);
    onChange(next);
  };

  const applyBulk = () => {
    if (!bulkField || !selectedIds.size) return;
    const nextValue: string | boolean =
      bulkColumns.find((column) => column.key === bulkField)?.kind === "bool"
        ? bulkValue === "true"
        : bulkValue;
    const next = rows.map((row, index) => {
      const clone = { ...row };
      if (selectedIds.has(rowId(row, index))) {
        applyValue(clone, bulkField, nextValue);
      }
      return clone;
    });
    onChange(next);
  };

  const duplicate = (index: number) =>
    onChange(duplicateRowAt(rows, index, groupField));

  const remove = (index: number) => {
    const next = removeRowAt(rows, index, groupField);
    if (next) onChange(next);
  };

  const move = (index: number, direction: -1 | 1) => {
    const next = moveRowAt(rows, index, direction);
    if (next) onChange(next);
  };

  const selectionGroupFields = Array.isArray(editor.selectionGroupFields)
    ? (editor.selectionGroupFields as unknown[]).map(String)
    : [];
  const groupChoices = selectionGroupFields.flatMap((field) => {
    const values = [
      ...new Set(rows.map((row) => String(row[field] ?? ""))),
    ].filter(Boolean);
    return values.length <= 12 ? values.map((value) => ({ field, value })) : [];
  });
  const bulkColumn = bulkColumns.find((column) => column.key === bulkField);
  const choiceRows = selectedRows.length
    ? selectedRows
    : rows.length
      ? [rows[0]]
      : [];
  const bulkChoiceValues = bulkColumn
    ? choiceRows
        .map((row) => choicesFor(bulkColumn, row))
        .reduce<string[]>(
          (common, values, index) =>
            index === 0
              ? values
              : common.filter((value) => values.includes(value)),
          [],
        )
    : [];
  const bulkSuggestions = Array.isArray(autocomplete[bulkField])
    ? (autocomplete[bulkField] as unknown[]).map(String)
    : [];
  const bulkListId = `bulk-${bulkField}`;

  return (
    <section
      className={`editor-card${editor.layout === "compact" ? " editor-card--compact" : ""}`}
    >
      <div className="editor-heading">
        <span>
          <TableProperties />
        </span>
        <div>
          <strong>{s.t(String(editor.titleKey ?? "rfi.editor.title"))}</strong>
          <small>
            {s.t(String(editor.rowCountKey ?? "rfi.editor.rowCount"), {
              count: rows.length,
            })}
          </small>
        </div>
        <div className="editor-heading-actions">
          {editor.detailsEnabled === true && (
            <Button
              variant="ghost"
              icon={detailsVisible ? <EyeOff /> : <Eye />}
              onClick={() => setDetailsVisible((visible) => !visible)}
            >
              {s.t(
                detailsVisible
                  ? "editor.action.hideDetails"
                  : "editor.action.showDetails",
              )}
            </Button>
          )}
        </div>
      </div>
      <PlanFilterBar
        searchTerm={searchTerm}
        filterMode={filterMode}
        onSearchTermChange={setSearchTerm}
        onFilterModeChange={setFilterMode}
        filteredCount={filteredRowIndices.length}
        totalCount={rows.length}
        s={s}
      />
      {editor.bulkEnabled === true && (
        <div className="bulk-editor" aria-label={s.t("editor.bulk.title")}>
          <PlanSelectionBar
            selectedCount={selectedIds.size}
            onSelectAll={selectAll}
            onSelectNone={selectNone}
            groupChoices={groupChoices}
            compact={editor.layout === "compact"}
            groupField={groupField}
            onSelectGroup={selectGroup}
            s={s}
          />
          <PlanBulkFields
            bulkField={bulkField}
            bulkValue={bulkValue}
            bulkColumns={bulkColumns}
            bulkColumn={bulkColumn}
            bulkChoiceValues={bulkChoiceValues}
            bulkSuggestions={bulkSuggestions}
            bulkListId={bulkListId}
            choiceLabel={choiceLabel}
            canApply={
              Boolean(selectedIds.size) && Boolean(bulkField) && bulkValue !== ""
            }
            onFieldChange={(field) => {
              setBulkField(field);
              setBulkValue("");
            }}
            onValueChange={setBulkValue}
            onApply={applyBulk}
            s={s}
          />
        </div>
      )}
      <AdjustableDataRegion
        s={s}
        className="editor-scroll"
        label={s.t(String(editor.titleKey ?? "rfi.editor.title"))}
      >
        <table
          className="editor-table"
          style={{ tableLayout: "fixed", width: `${tableWidth}px`, minWidth: 0 }}
        >
          <colgroup>
            {editor.selectionEnabled === true && (
              <col className="selection-column" style={{ width: 52 }} />
            )}
            {visibleColumns.map((column) => (
              <col key={column.key} style={{ width: columnWidth(column) }} />
            ))}
            {(editor.reorderable === true || editor.duplicateEnabled === true) && (
              <col className="editor-actions-column" style={{ width: 132 }} />
            )}
          </colgroup>
          <PlanTableHead
            showSelection={editor.selectionEnabled === true}
            showActions={
              editor.reorderable === true || editor.duplicateEnabled === true
            }
            allSelected={
              filteredRowIndices.length > 0 &&
              filteredRowIndices.every((index) =>
                selectedIds.has(rowId(rows[index], index)),
              )
            }
            onSelectAll={selectAll}
            onSelectNone={selectNone}
            visibleColumns={visibleColumns}
            columnWidth={columnWidth}
            headerId={headerId}
            onColumnWidthChange={(columnKey, nextWidth) =>
              setColumnWidths((current) => ({
                ...current,
                [`${widthScope}:${columnKey}`]: nextWidth,
              }))
            }
            s={s}
          />
          <tbody>
            {filteredRowIndices.map((rowIndex) => {
              const row = rows[rowIndex];
              const sheet = String(row._sheet ?? "");
              const groupCount = rows.filter(
                (item) => item[groupField] === row[groupField],
              ).length;
              const id = rowId(row, rowIndex);
              return (
                <tr
                  key={id}
                  className={selectedIds.has(id) ? "selected-row" : ""}
                  tabIndex={0}
                  onKeyDown={(event) => {
                    if (
                      event.key === " " &&
                      event.target === event.currentTarget
                    ) {
                      event.preventDefault();
                      if (editor.selectionEnabled === true) {
                        toggleRow(id);
                      } else if (
                        typeof row.enabled === "boolean" &&
                        row.selectable !== false
                      ) {
                        update(rowIndex, "enabled", !row.enabled);
                      }
                    }
                  }}
                >
                  {editor.selectionEnabled === true && (
                    <td className="selection-column">
                      <input
                        aria-label={s.t("editor.selectRow", {
                          row: rowIndex + 1,
                        })}
                        type="checkbox"
                        checked={selectedIds.has(id)}
                        onChange={() => toggleRow(id)}
                      />
                      {groupCount > 1 && (
                        <button
                          type="button"
                          title={s.t("editor.selectGroup")}
                          onClick={() =>
                            selectGroup(groupField, row[groupField])
                          }
                        >
                          {groupCount}
                        </button>
                      )}
                    </td>
                  )}
                  {visibleColumns.map((column) => {
                    if (column.sheets && !column.sheets.includes(sheet)) {
                      return (
                        <td key={column.key} className="not-applicable">
                          —
                        </td>
                      );
                    }
                    const text = String(row[column.key] ?? "");
                    const displayText =
                      column.display === "localizedKey"
                        ? s.t(text)
                        : column.key === "status" && text
                          ? s.t(`print.status.${text}`)
                          : column.key === "worksheetVisibility" && text
                            ? s.t(`print.worksheet.visibility.${text}`)
                            : displayEditorValue(column, text);
                    const choices = choicesFor(column, row);
                    const suggestions = Array.isArray(autocomplete[column.key])
                      ? (autocomplete[column.key] as unknown[]).map(String)
                      : [];
                    const listId = `editor-${rowIndex}-${column.key}`;
                    const isScanCode =
                      editor.kind === "rfiScan" && column.key === "code";
                    const duplicateScanCode =
                      isScanCode &&
                      rows.some(
                        (other, index) =>
                          index !== rowIndex && other.code === row.code,
                      );
                    const scanCodeErrorKey = isScanCode
                      ? !isRfiCodeCandidate(row.code)
                        ? "error.rfi.fastScan.codeInvalid"
                        : duplicateScanCode
                          ? "error.rfi.fastScan.targetExists"
                          : undefined
                      : undefined;
                    return (
                      <td
                        key={column.key}
                        className={
                          column.kind === "bool" ? "center" : undefined
                        }
                      >
                        {column.kind === "readonly" ? (
                          <span
                            className="readonly-cell"
                            title={displayText || text}
                          >
                            {displayText}
                          </span>
                        ) : column.kind === "bool" ? (
                          <input
                            aria-label={s.t(column.labelKey)}
                            type="checkbox"
                            checked={Boolean(row[column.key])}
                            disabled={
                              column.disabledWhen !== undefined &&
                              row[column.disabledWhen.field] ===
                                column.disabledWhen.equals
                            }
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.checked)
                            }
                          />
                        ) : column.kind === "choice" ? (
                          <select
                            aria-label={s.t(column.labelKey)}
                            title={text}
                            value={text}
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.value)
                            }
                          >
                            {choices.map((choice) => (
                              <option key={choice} value={choice}>
                                {choiceLabel(column.key, choice, column)}
                              </option>
                            ))}
                          </select>
                        ) : column.kind === "compactDate" ? (
                          <CompactDateInput
                            value={text}
                            label={s.t(column.labelKey)}
                            s={s}
                            onChange={(value) =>
                              update(rowIndex, column.key, value)
                            }
                          />
                        ) : column.kind === "multiline" ? (
                          <textarea
                            rows={2}
                            value={text}
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.value)
                            }
                          />
                        ) : (
                          <>
                            <input
                              aria-label={s.t(column.labelKey)}
                              aria-invalid={
                                scanCodeErrorKey ? true : undefined
                              }
                              aria-describedby={
                                scanCodeErrorKey ? `${listId}-error` : undefined
                              }
                              type={column.kind === "int" ? "number" : "text"}
                              name={`${column.key}-${rowIndex}`}
                              autoComplete={isScanCode ? "off" : undefined}
                              spellCheck={isScanCode ? false : undefined}
                              translate={isScanCode ? "no" : undefined}
                              value={text}
                              list={suggestions.length ? listId : undefined}
                              onChange={(event) =>
                                update(rowIndex, column.key, event.target.value)
                              }
                            />
                            {suggestions.length > 0 && (
                              <datalist id={listId}>
                                {suggestions.map((suggestion) => (
                                  <option key={suggestion} value={suggestion} />
                                ))}
                              </datalist>
                            )}
                            {scanCodeErrorKey && (
                              <small
                                id={`${listId}-error`}
                                className="editor-field-error"
                                role="alert"
                              >
                                {s.t(scanCodeErrorKey)}
                              </small>
                            )}
                          </>
                        )}
                      </td>
                    );
                  })}
                  {(editor.reorderable === true ||
                    editor.duplicateEnabled === true) && (
                    <PlanRowActions
                      rowIndex={rowIndex}
                      rowCount={rows.length}
                      groupCount={groupCount}
                      reorderable={editor.reorderable === true}
                      duplicateEnabled={editor.duplicateEnabled === true}
                      onMove={move}
                      onDuplicate={duplicate}
                      onRemove={remove}
                      s={s}
                    />
                  )}
                </tr>
              );
            })}
            {filteredRowIndices.length === 0 && (
              <tr>
                <td
                  colSpan={
                    visibleColumns.length +
                    Number(editor.selectionEnabled === true) +
                    Number(editor.reorderable === true || editor.duplicateEnabled === true)
                  }
                  className="empty-filter-cell"
                >
                  {s.t("plan.empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </AdjustableDataRegion>
    </section>
  );
}
