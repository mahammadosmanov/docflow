import type { Strings } from "../i18n";
import type { ComputeOp, FieldCompute, TemplateField } from "../types";

const OPERATIONS: ComputeOp[] = ["sum", "difference", "offset", "round", "copy", "concat"];

/** The closed set of computations a field can be made from (ADR 0058). */
export function TemplateCompute({
  s,
  field,
  others,
  onChange,
}: {
  s: Strings;
  field: TemplateField;
  others: TemplateField[];
  onChange: (compute: FieldCompute) => void;
}) {
  const compute: FieldCompute = field.compute ?? { op: "sum", inputs: [] };
  const numeric = compute.op !== "copy" && compute.op !== "concat";
  const toggle = (key: string) =>
    onChange({
      ...compute,
      inputs: compute.inputs.includes(key)
        ? compute.inputs.filter((item) => item !== key)
        : [...compute.inputs, key],
    });
  return (
    <div className="template-editor-grid">
      <label className="field">
        <span>{s.t("templates.editor.compute.op")}</span>
        <select
          value={compute.op}
          onChange={(event) =>
            onChange({ op: event.target.value as ComputeOp, inputs: compute.inputs })
          }
        >
          {OPERATIONS.map((op) => (
            <option key={op} value={op}>
              {s.t(`templates.editor.compute.op.${op}`)}
            </option>
          ))}
        </select>
      </label>
      <fieldset className="field">
        <legend>{s.t("templates.editor.compute.inputs")}</legend>
        {others.length === 0 && <small>{s.t("templates.editor.compute.noInputs")}</small>}
        {others.map((other) => (
          <label className="toggle-inline" key={other.key}>
            <input
              type="checkbox"
              checked={compute.inputs.includes(other.key)}
              onChange={() => toggle(other.key)}
            />
            {other.label || other.key}
          </label>
        ))}
        <small>{s.t(`templates.editor.compute.help.${compute.op}`)}</small>
      </fieldset>
      {compute.op === "offset" && (
        <label className="field">
          <span>{s.t("templates.editor.compute.by")}</span>
          <input
            value={compute.by ?? ""}
            placeholder="-0.6"
            onChange={(event) => onChange({ ...compute, by: event.target.value })}
          />
        </label>
      )}
      {numeric && (
        <label className="field">
          <span>{s.t("templates.editor.compute.decimals")}</span>
          <input
            type="number"
            min={0}
            max={6}
            value={compute.decimals ?? ""}
            onChange={(event) =>
              onChange({
                ...compute,
                decimals: event.target.value === "" ? undefined : Number(event.target.value),
              })
            }
          />
        </label>
      )}
      {compute.op === "concat" && (
        <label className="field">
          <span>{s.t("templates.editor.compute.separator")}</span>
          <input
            value={compute.separator ?? ""}
            maxLength={20}
            onChange={(event) => onChange({ ...compute, separator: event.target.value })}
          />
        </label>
      )}
    </div>
  );
}
