import {
  CompactDateInput,
} from "./CompactDateInput";
import { type Strings } from "../i18n";
import { type EditorColumn } from "./editorRules";

export function PlanCell({
  column,
  row,
  rowIndex,
  text,
  displayText,
  choices,
  choiceLabel,
  suggestions,
  listId,
  isScanCode,
  scanCodeErrorKey,
  s,
  onChange,
}: {
  column: EditorColumn;
  row: Record<string, unknown>;
  rowIndex: number;
  text: string;
  displayText: string;
  choices: string[];
  choiceLabel: (field: string, value: string, column: EditorColumn) => string;
  suggestions: string[];
  listId: string;
  isScanCode: boolean;
  scanCodeErrorKey: string | undefined;
  s: Strings;
  onChange: (value: string | boolean) => void;
}) {
  return (
    <>
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
              onChange(event.target.checked)
            }
          />
        ) : column.kind === "choice" ? (
          <select
            aria-label={s.t(column.labelKey)}
            title={text}
            value={text}
            onChange={(event) =>
              onChange(event.target.value)
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
              onChange(value)
            }
          />
        ) : column.kind === "multiline" ? (
          <textarea
            rows={2}
            aria-label={s.t(column.labelKey)}
            value={text}
            onChange={(event) =>
              onChange(event.target.value)
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
                onChange(event.target.value)
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
    </>
  );
}
