import { type Strings } from "../i18n";

/** Selection controls above the bulk editor: count, select all or none, and per-group picks. */
export function PlanSelectionBar({
  selectedCount,
  onSelectAll,
  onSelectNone,
  groupChoices,
  compact,
  groupField,
  onSelectGroup,
  s,
}: {
  selectedCount: number;
  onSelectAll: () => void;
  onSelectNone: () => void;
  groupChoices: { field: string; value: string }[];
  compact: boolean;
  groupField: string;
  onSelectGroup: (field: string, value: unknown) => void;
  s: Strings;
}) {
  return (
    <div className="bulk-selection">
      <strong>
        {s.t("editor.selectedCount", { count: selectedCount })}
      </strong>
      <button type="button" onClick={onSelectAll}>
        {s.t("editor.selectAll")}
      </button>
      <button type="button" onClick={onSelectNone}>
        {s.t("editor.selectNone")}
      </button>
      {groupChoices.map(({ field, value }) => (
        <button
          type="button"
          key={`${field}-${value}`}
          title={
            compact && field === groupField
              ? value
              : undefined
          }
          onClick={() => onSelectGroup(field, value)}
        >
          {compact && field === groupField
            ? (value.match(/(\d+)\s*$/)?.[1] ?? value)
            : value}
        </button>
      ))}
    </div>
  );
}
