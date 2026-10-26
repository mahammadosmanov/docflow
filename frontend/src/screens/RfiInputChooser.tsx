import { Button } from "../components/ui";
import type { Strings } from "../i18n";

/** Checkboxes for the RFI numbers typed for batch print; unchecked numbers are skipped. */
export function RfiInputChooser({
  inputs,
  excluded,
  s,
  onSelectAll,
  onSelectNone,
  onToggle,
}: {
  inputs: string[];
  excluded: string[];
  s: Strings;
  onSelectAll: () => void;
  onSelectNone: () => void;
  onToggle: (input: string, checked: boolean) => void;
}) {
  if (inputs.length === 0) return null;
  return (
    <fieldset className="rfi-print-inputs">
      <legend>{s.t("print.rfi.inputs")}</legend>
      <div className="rfi-print-actions">
        <Button variant="ghost" onClick={onSelectAll}>
          {s.t("action.selectAll")}
        </Button>
        <Button variant="ghost" onClick={onSelectNone}>
          {s.t("action.selectNone")}
        </Button>
      </div>
      {inputs.map((input) => (
        <label className="rfi-print-row" key={input}>
          <input
            type="checkbox"
            checked={!excluded.includes(input)}
            onChange={(event) => onToggle(input, event.target.checked)}
          />
          <span>{input}</span>
        </label>
      ))}
    </fieldset>
  );
}
