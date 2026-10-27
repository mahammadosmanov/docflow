import { Button } from "../components/ui";
import { type Strings } from "../i18n";
import { CompactDateInput } from "./CompactDateInput";
import { type EditorColumn } from "./editorRules";

/** Field, value, and apply controls of the bulk editor. */
export function PlanBulkFields({
  bulkField,
  bulkValue,
  bulkColumns,
  bulkColumn,
  bulkChoiceValues,
  bulkSuggestions,
  bulkListId,
  choiceLabel,
  canApply,
  onFieldChange,
  onValueChange,
  onApply,
  s,
}: {
  bulkField: string;
  bulkValue: string;
  bulkColumns: EditorColumn[];
  bulkColumn: EditorColumn | undefined;
  bulkChoiceValues: string[];
  bulkSuggestions: string[];
  bulkListId: string;
  choiceLabel: (field: string, value: string, column?: EditorColumn) => string;
  canApply: boolean;
  onFieldChange: (field: string) => void;
  onValueChange: (value: string) => void;
  onApply: () => void;
  s: Strings;
}) {
  return (
    <div className="bulk-fields">
      <select
        aria-label={s.t("editor.bulk.field")}
        value={bulkField}
        onChange={(event) => {
          onFieldChange(event.target.value);
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
          onChange={(event) => onValueChange(event.target.value)}
        >
          <option value="">{s.t("editor.bulk.choose")}</option>
          <option value="true">{s.t("editor.bool.true")}</option>
          <option value="false">{s.t("editor.bool.false")}</option>
        </select>
      ) : bulkColumn?.kind === "choice" ? (
        <select
          aria-label={s.t("editor.bulk.value")}
          value={bulkValue}
          onChange={(event) => onValueChange(event.target.value)}
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
          onChange={onValueChange}
        />
      ) : (
        <>
          <input
            aria-label={s.t("editor.bulk.value")}
            type={bulkColumn?.kind === "int" ? "number" : "text"}
            value={bulkValue}
            list={bulkSuggestions.length ? bulkListId : undefined}
            onChange={(event) => onValueChange(event.target.value)}
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
        disabled={!canApply}
        onClick={onApply}
      >
        {s.t("editor.bulk.apply")}
      </Button>
    </div>
  );
}
