import { useEffect, useState } from "react";

type PlanRow = Record<string, unknown>;

/** Stable id for a plan row: its `_rowId`, or a group-and-position fallback. */
export function planRowId(row: PlanRow, index: number, groupField: string): string {
  return String(row._rowId ?? `${row[groupField] ?? "row"}-${index}`);
}

/** Row selection for the plan table; drops ids for rows that no longer exist. */
export function usePlanSelection({
  rows,
  groupField,
  filteredRowIndices,
}: {
  rows: PlanRow[];
  groupField: string;
  filteredRowIndices: number[];
}) {
  const rowId = (row: PlanRow, index: number) => planRowId(row, index, groupField);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    const validIds = new Set(rows.map((row, index) => rowId(row, index)));
    setSelectedIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
    // `rowId` only depends on groupField, which is listed below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, groupField]);

  const selectedRows = rows.filter((row, index) => selectedIds.has(rowId(row, index)));

  const selectAll = () =>
    setSelectedIds(new Set(filteredRowIndices.map((index) => rowId(rows[index], index))));
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

  return {
    rowId,
    selectedIds,
    selectedRows,
    selectAll,
    selectNone,
    toggleRow,
    selectGroup,
  };
}
