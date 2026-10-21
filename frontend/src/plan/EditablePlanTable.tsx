import {
  AdjustableDataRegion,
  ColumnResizeHandle,
  clampColumnWidth,
} from "../components/AdjustableDataRegion";
import {
  ArrowDown,
  ArrowUp,
  CalendarDays,
  Copy,
  Eye,
  EyeOff,
  TableProperties,
  Trash2,
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
  type Plan,
} from "../types";

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

export const EDITOR_DATE_MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;

export type EditorDateParts = {
  day: number;
  month: number;
  year: number;
};

export function parseEditorDate(value: string): EditorDateParts | null {
  const match = value.trim().match(/^(\d{1,2})-([A-Za-z]{3})-(\d{2}|\d{4})$/);
  if (!match) return null;
  const day = Number(match[1]);
  const month =
    EDITOR_DATE_MONTHS.findIndex(
      (name) => name.toLowerCase() === match[2].toLowerCase(),
    ) + 1;
  const rawYear = Number(match[3]);
  const year = rawYear < 100 ? 2000 + rawYear : rawYear;
  if (!month || year < 2000 || year > 2068) return null;
  const maxDay = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return day >= 1 && day <= maxDay ? { day, month, year } : null;
}

export function formatEditorDate({ day, month, year }: EditorDateParts): string {
  return `${String(day).padStart(2, "0")}-${EDITOR_DATE_MONTHS[month - 1]}-${String(year).slice(-2)}`;
}

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

export function CompactDateInput({
  value,
  label,
  s,
  onChange,
}: {
  value: string;
  label: string;
  s: Strings;
  onChange: (value: string) => void;
}) {
  const currentYear = new Date().getFullYear();
  const parsed = parseEditorDate(value);
  const [draft, setDraft] = useState<{
    day: number | "";
    month: number | "";
    year: number;
  }>(() => parsed ?? { day: "", month: "", year: currentYear });
  const [yearVisible, setYearVisible] = useState(false);

  useEffect(() => {
    const next = parseEditorDate(value);
    setDraft(next ?? { day: "", month: "", year: currentYear });
  }, [value, currentYear]);

  const commit = (next: typeof draft) => {
    let normalized = next;
    if (next.day && next.month) {
      const maxDay = new Date(Date.UTC(next.year, next.month, 0)).getUTCDate();
      normalized = { ...next, day: Math.min(next.day, maxDay) };
      onChange(formatEditorDate(normalized as EditorDateParts));
    }
    setDraft(normalized);
  };
  const maxDay = draft.month
    ? new Date(Date.UTC(draft.year, draft.month, 0)).getUTCDate()
    : 31;
  const monthLabels = useMemo(
    () =>
      EDITOR_DATE_MONTHS.map((_, index) =>
        new Intl.DateTimeFormat(s.locale, { month: "short" }).format(
          new Date(Date.UTC(2024, index, 1)),
        ),
      ),
    [s.locale],
  );

  return (
    <div className="compact-date-cell" title={value || undefined}>
      <select
        className="compact-date-day"
        aria-label={`${label} — ${s.t("rfi.date.day")}`}
        value={draft.day}
        onChange={(event) =>
          commit({ ...draft, day: Number(event.target.value) || "" })
        }
      >
        <option value="">--</option>
        {Array.from({ length: maxDay }, (_, index) => index + 1).map((day) => (
          <option key={day} value={day}>
            {String(day).padStart(2, "0")}
          </option>
        ))}
      </select>
      <select
        className="compact-date-month"
        aria-label={`${label} — ${s.t("rfi.date.month")}`}
        value={draft.month}
        onChange={(event) =>
          commit({ ...draft, month: Number(event.target.value) || "" })
        }
      >
        <option value="">---</option>
        {monthLabels.map((month, index) => (
          <option key={EDITOR_DATE_MONTHS[index]} value={index + 1}>
            {month.replace(".", "")}
          </option>
        ))}
      </select>
      <button
        type="button"
        className={yearVisible ? "active" : ""}
        aria-label={`${label} — ${s.t("rfi.date.changeYear")}`}
        title={s.t("rfi.date.changeYear")}
        onClick={() => setYearVisible((visible) => !visible)}
      >
        <CalendarDays />
      </button>
      {yearVisible && (
        <select
          autoFocus
          className="compact-date-year"
          aria-label={`${label} — ${s.t("rfi.date.year")}`}
          value={draft.year}
          onBlur={() => setYearVisible(false)}
          onChange={(event) => {
            commit({ ...draft, year: Number(event.target.value) });
            setYearVisible(false);
          }}
        >
          {Array.from({ length: 69 }, (_, index) => 2000 + index).map(
            (year) => (
              <option key={year} value={year}>
                {year}
              </option>
            ),
          )}
        </select>
      )}
    </div>
  );
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
  const directChoices =
    editor.choices && typeof editor.choices === "object"
      ? (editor.choices as Record<string, unknown>)
      : {};
  const dependentChoices =
    editor.choicesByValue && typeof editor.choicesByValue === "object"
      ? (editor.choicesByValue as Record<string, unknown>)
      : {};
  const autocomplete =
    editor.autocomplete && typeof editor.autocomplete === "object"
      ? (editor.autocomplete as Record<string, unknown>)
      : {};
  const choiceLabels =
    editor.choiceLabels && typeof editor.choiceLabels === "object"
      ? (editor.choiceLabels as Record<string, unknown>)
      : {};
  const rowId = (row: Record<string, unknown>, index: number) =>
    String(row._rowId ?? `${row[groupField] ?? "row"}-${index}`);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
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
  const selectedRows = rows.filter((row, index) =>
    selectedIds.has(rowId(row, index)),
  );

  const [searchTerm, setSearchTerm] = useState("");
  const [filterMode, setFilterMode] = useState<"all" | "changes" | "review">(
    "all",
  );

  const normalizedSearch = searchTerm.trim().toLowerCase();
  const filteredRowIndices = useMemo(() => {
    return rows
      .map((row, index) => {
        if (filterMode === "changes") {
          const isChange =
            (row.proposedStatus && row.proposedStatus !== row.currentStatus) ||
            row.enabled === true;
          if (!isChange) return -1;
        } else if (filterMode === "review") {
          const isReview =
            row.disposition === "manual_review" ||
            Boolean(row.reasonKey) ||
            row.selectable === false ||
            row.enabled === false;
          if (!isReview) return -1;
        }
        if (normalizedSearch) {
          const matches = Object.values(row).some((val) =>
            typeof val === "string" || typeof val === "number"
              ? String(val).toLowerCase().includes(normalizedSearch)
              : false,
          );
          if (!matches) return -1;
        }
        return index;
      })
      .filter((index) => index !== -1);
  }, [rows, filterMode, normalizedSearch]);

  useEffect(() => {
    const validIds = new Set(rows.map((row, index) => rowId(row, index)));
    setSelectedIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [rows, groupField]);

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

  const choicesFor = (
    column: EditorColumn,
    row: Record<string, unknown>,
  ): string[] => {
    const direct = directChoices[column.key];
    if (Array.isArray(direct)) return direct.map(String);
    const config = dependentChoices[column.key];
    if (!config || typeof config !== "object") return [];
    const typed = config as Record<string, unknown>;
    const values = typed.values;
    const dependency = String(typed.dependsOn ?? "");
    if (!values || typeof values !== "object") return [];
    const options = (values as Record<string, unknown>)[
      String(row[dependency] ?? "")
    ];
    return Array.isArray(options) ? options.map(String) : [];
  };

  const applyValue = (
    row: Record<string, unknown>,
    key: string,
    value: string | boolean,
  ) => {
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
  };

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

  const selectAll = () =>
    setSelectedIds(
      new Set(filteredRowIndices.map((index) => rowId(rows[index], index))),
    );
  const selectNone = () => setSelectedIds(new Set());
  const toggleRow = (id: string) =>
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const selectGroup = (field: string, value: unknown) =>
    setSelectedIds(
      new Set(
        rows
          .map((row, index) => ({ row, id: rowId(row, index) }))
          .filter(({ row }) => row[field] === value)
          .map(({ id }) => id),
      ),
    );

  const duplicate = (index: number) => {
    const source = rows[index];
    const group = source[groupField];
    let insertAt = index + 1;
    while (insertAt < rows.length && rows[insertAt][groupField] === group) {
      insertAt += 1;
    }
    const clone = {
      ...source,
      _rowId: `${String(source._rowId ?? group)}-copy-${Date.now()}`,
    };
    const next = [...rows];
    next.splice(insertAt, 0, clone);
    onChange(next);
  };

  const remove = (index: number) => {
    const group = rows[index][groupField];
    if (rows.filter((row) => row[groupField] === group).length <= 1) return;
    onChange(rows.filter((_, rowIndex) => rowIndex !== index));
  };

  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= rows.length) return;
    const next = [...rows];
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
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
      <div className="table-filter-bar">
        <div className="table-search-box">
          <input
            type="search"
            aria-label={s.t("filter.searchPlaceholder")}
            placeholder={s.t("filter.searchPlaceholder")}
            value={searchTerm}
            onChange={(event) => setSearchTerm(event.target.value)}
          />
        </div>
        <div className="table-filter-tabs">
          <button
            type="button"
            className={`filter-tab ${filterMode === "all" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("all")}
          >
            {s.t("filter.all")}
          </button>
          <button
            type="button"
            className={`filter-tab ${filterMode === "changes" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("changes")}
          >
            {s.t("filter.changes")}
          </button>
          <button
            type="button"
            className={`filter-tab ${filterMode === "review" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("review")}
          >
            {s.t("filter.manualReview")}
          </button>
        </div>
        {(searchTerm !== "" || filterMode !== "all") && (
          <span className="table-filter-count">
            {s.t("table.filterCount", {
              filtered: filteredRowIndices.length,
              total: rows.length,
            })}
          </span>
        )}
      </div>
      {editor.bulkEnabled === true && (
        <div className="bulk-editor" aria-label={s.t("editor.bulk.title")}>
          <div className="bulk-selection">
            <strong>
              {s.t("editor.selectedCount", { count: selectedIds.size })}
            </strong>
            <button type="button" onClick={selectAll}>
              {s.t("editor.selectAll")}
            </button>
            <button type="button" onClick={selectNone}>
              {s.t("editor.selectNone")}
            </button>
            {groupChoices.map(({ field, value }) => (
              <button
                type="button"
                key={`${field}-${value}`}
                title={
                  editor.layout === "compact" && field === groupField
                    ? value
                    : undefined
                }
                onClick={() => selectGroup(field, value)}
              >
                {editor.layout === "compact" && field === groupField
                  ? (value.match(/(\d+)\s*$/)?.[1] ?? value)
                  : value}
              </button>
            ))}
          </div>
          <div className="bulk-fields">
            <select
              aria-label={s.t("editor.bulk.field")}
              value={bulkField}
              onChange={(event) => {
                setBulkField(event.target.value);
                setBulkValue("");
              }}
            >
              {bulkColumns.map((column) => (
                <option key={column.key} value={column.key}>
                  {s.t(column.labelKey)}
                </option>
              ))}
            </select>
            {bulkColumn?.kind === "bool" ? (
              <select
                aria-label={s.t("editor.bulk.value")}
                value={bulkValue}
                onChange={(event) => setBulkValue(event.target.value)}
              >
                <option value="">{s.t("editor.bulk.choose")}</option>
                <option value="true">{s.t("editor.bool.true")}</option>
                <option value="false">{s.t("editor.bool.false")}</option>
              </select>
            ) : bulkColumn?.kind === "choice" ? (
              <select
                aria-label={s.t("editor.bulk.value")}
                value={bulkValue}
                onChange={(event) => setBulkValue(event.target.value)}
              >
                <option value="">{s.t("editor.bulk.choose")}</option>
                {bulkChoiceValues.map((choice) => (
                  <option key={choice} value={choice}>
                    {choiceLabel(bulkField, choice, bulkColumn)}
                  </option>
                ))}
              </select>
            ) : bulkColumn?.kind === "compactDate" ? (
              <CompactDateInput
                key={bulkField}
                value={bulkValue}
                label={s.t(bulkColumn.labelKey)}
                s={s}
                onChange={setBulkValue}
              />
            ) : (
              <>
                <input
                  aria-label={s.t("editor.bulk.value")}
                  type={bulkColumn?.kind === "int" ? "number" : "text"}
                  value={bulkValue}
                  list={bulkSuggestions.length ? bulkListId : undefined}
                  onChange={(event) => setBulkValue(event.target.value)}
                />
                {bulkSuggestions.length > 0 && (
                  <datalist id={bulkListId}>
                    {bulkSuggestions.map((suggestion) => (
                      <option key={suggestion} value={suggestion} />
                    ))}
                  </datalist>
                )}
              </>
            )}
            <Button
              variant="secondary"
              disabled={!selectedIds.size || !bulkField || bulkValue === ""}
              onClick={applyBulk}
            >
              {s.t("editor.bulk.apply")}
            </Button>
          </div>
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
          <thead>
            <tr>
              {editor.selectionEnabled === true && (
                <th className="selection-column">
                  <input
                    aria-label={s.t("editor.selectAll")}
                    type="checkbox"
                    checked={
                      filteredRowIndices.length > 0 &&
                      filteredRowIndices.every((index) =>
                        selectedIds.has(rowId(rows[index], index)),
                      )
                    }
                    onChange={(event) =>
                      event.target.checked ? selectAll() : selectNone()
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
                        setColumnWidths((current) => ({
                          ...current,
                          [`${widthScope}:${column.key}`]: nextWidth,
                        }))
                      }
                    />
                  </th>
                );
              })}
              {(editor.reorderable === true ||
                editor.duplicateEnabled === true) && (
                <th
                  className="editor-actions-header"
                  aria-label={s.t("editor.rowActions")}
                />
              )}
            </tr>
          </thead>
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
                    <td className="editor-row-actions">
                      {editor.reorderable === true && (
                        <>
                          <button
                            type="button"
                            aria-label={s.t("action.moveUp")}
                            title={s.t("action.moveUp")}
                            disabled={rowIndex === 0}
                            onClick={() => move(rowIndex, -1)}
                          >
                            <ArrowUp />
                          </button>
                          <button
                            type="button"
                            aria-label={s.t("action.moveDown")}
                            title={s.t("action.moveDown")}
                            disabled={rowIndex === rows.length - 1}
                            onClick={() => move(rowIndex, 1)}
                          >
                            <ArrowDown />
                          </button>
                        </>
                      )}
                      {editor.duplicateEnabled === true && (
                        <>
                          <button
                            type="button"
                            title={s.t("rfi.action.duplicate")}
                            onClick={() => duplicate(rowIndex)}
                          >
                            <Copy />
                          </button>
                          <button
                            type="button"
                            title={s.t("rfi.action.removeDuplicate")}
                            disabled={groupCount <= 1}
                            onClick={() => remove(rowIndex)}
                          >
                            <Trash2 />
                          </button>
                        </>
                      )}
                    </td>
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

export function isCanonicalRfiCode(value: unknown): value is string {
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
