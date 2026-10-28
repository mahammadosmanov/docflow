import {
  Switch,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  type ParamDef,
} from "../types";
import {
  MultiFileField,
  PathField,
} from "./SchemaFieldKinds";

export function SchemaField({
  bridge,
  operationId,
  param,
  value,
  values,
  s,
  onChange,
  invalid,
}: {
  bridge: Bridge;
  operationId: string;
  param: ParamDef;
  value: unknown;
  values: Record<string, unknown>;
  s: Strings;
  onChange: (value: unknown) => void;
  invalid?: boolean;
}) {
  if (
    !Object.entries(param.visibleWhen ?? {}).every(
      ([name, expected]) => values[name] === expected,
    )
  ) {
    return null;
  }
  if (param.kind === "bool") {
    return (
      <label className="toggle-row">
        <span>
          <strong>{s.t(param.labelKey)}</strong>
          {param.helpKey && <small>{s.t(param.helpKey)}</small>}
        </span>
        <Switch
          checked={Boolean(value)}
          onChange={onChange}
          ariaLabel={s.t(param.labelKey)}
        />
      </label>
    );
  }
  if (param.kind === "choice") {
    const currentChoice = String(value ?? "");
    const choiceIsTemporarilyUnavailable =
      currentChoice.length > 0 && !param.choices.includes(currentChoice);
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <select

          aria-invalid={invalid || undefined}
          value={currentChoice}
          onChange={(event) => onChange(event.target.value)}
        >
          {choiceIsTemporarilyUnavailable && (
            <option value={currentChoice}>{currentChoice}</option>
          )}
          {param.choices.map((choice) => (
            <option key={choice} value={choice}>
              {param.choiceLabels?.[choice]
                ? s.t(param.choiceLabels[choice])
                : choice}
            </option>
          ))}
        </select>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  if (param.kind === "files") {
    return (
      <MultiFileField
        bridge={bridge}
        operationId={operationId}
        param={param}
        value={value}
        s={s}
        onChange={onChange}
      />
    );
  }
  if (param.kind === "sheetCounts") {
    const counts =
      value && typeof value === "object"
        ? (value as Record<string, number>)
        : {};
    const update = (choice: string, next: number) =>
      onChange({ ...counts, [choice]: Math.max(0, Math.min(100, next)) });
    const total = param.choices.reduce(
      (sum, choice) => sum + Number(counts[choice] ?? 0),
      0,
    );
    return (
      <div className="field sheet-count-field">
        <span>{s.t(param.labelKey)}</span>
        <div className="sheet-count-grid">
          {param.choices.map((choice) => {
            const count = Number(counts[choice] ?? 0);
            return (
              <div
                className={`sheet-count ${count > 0 ? "active" : ""}`}
                key={choice}
              >
                <strong>{choice}</strong>
                <div>
                  <button
                    type="button"
                    disabled={count <= 0}
                    aria-label={`${choice} ${s.t("action.decrease")}`}
                    onClick={() => update(choice, count - 1)}
                  >
                    −
                  </button>
                  <span>{count}</span>
                  <button
                    type="button"
                    disabled={count >= 100}
                    aria-label={`${choice} ${s.t("action.increase")}`}
                    onClick={() => update(choice, count + 1)}
                  >
                    +
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <small>{s.t("rfi.sheetCounts.total", { count: total })}</small>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </div>
    );
  }
  if (param.kind === "longText") {
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <textarea

          aria-invalid={invalid || undefined}
          aria-label={s.t(param.labelKey)}
          rows={8}
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  return (
    <PathField
      bridge={bridge}
      operationId={operationId}
      param={param}
      value={value}
      invalid={invalid}
      s={s}
      onChange={onChange}
    />
  );
}
