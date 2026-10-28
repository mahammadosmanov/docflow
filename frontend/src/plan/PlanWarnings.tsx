import {
  CircleAlert,
  Eye,
} from "lucide-react";
import {
  useState,
} from "react";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";
import {
  canonicalRfiInText,
} from "./workbookCellChanges";

export function PlanWarnings({
  warnings,
  focusableCodes,
  onFocusCode,
  s,
}: {
  warnings: Plan["warnings"];
  focusableCodes: string[];
  onFocusCode?: (code: string) => void;
  s: Strings;
}) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const types = [...new Set(warnings.map((warning) => warning.key))];
  const normalizedQuery = query.trim().toLocaleLowerCase(s.locale);
  const visible = warnings.filter((warning) => {
    if (type !== "all" && warning.key !== type) return false;
    if (!normalizedQuery) return true;
    return `${warning.subject} ${s.t(warning.key)}`
      .toLocaleLowerCase(s.locale)
      .includes(normalizedQuery);
  });
  const focusable = new Set(focusableCodes);
  return (
    <section
      id="signed-warning-list"
      className="warning-review"
      aria-labelledby="warning-review-title"
    >
      <div className="warning-review-heading">
        <div>
          <CircleAlert aria-hidden="true" />
          <strong id="warning-review-title">{s.t("ui.warnings")}</strong>
          <span>{warnings.length}</span>
        </div>
        {warnings.length >= 5 && (
          <div className="warning-review-filters">
            <input
              type="search"
              aria-label={s.t("plan.warnings.search")}
              placeholder={s.t("plan.warnings.search")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <select
              aria-label={s.t("plan.warnings.type")}
              value={type}
              onChange={(event) => setType(event.target.value)}
            >
              <option value="all">{s.t("plan.warnings.all")}</option>
              {types.map((key) => (
                <option key={key} value={key}>
                  {s.t(key)}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>
      <div className="warning-review-list" role="list">
        {visible.map((warning, index) => {
          const code = canonicalRfiInText(warning.subject);
          const canFocus = Boolean(code && focusable.has(code) && onFocusCode);
          return (
            <div
              key={`${warning.key}-${warning.subject}-${index}`}
              role="listitem"
            >
              <div>
                {warning.subject && <strong>{warning.subject}</strong>}
                <span>{s.t(warning.key)}</span>
              </div>
              {canFocus && code && (
                <button type="button" onClick={() => onFocusCode?.(code)}>
                  <Eye aria-hidden="true" />
                  {s.t("plan.warnings.focus")}
                </button>
              )}
            </div>
          );
        })}
        {visible.length === 0 && <p>{s.t("plan.warnings.none")}</p>}
      </div>
      {warnings.length >= 5 && (
        <small>
          {s.t("plan.warnings.showing", {
            shown: visible.length,
            count: warnings.length,
          })}
        </small>
      )}
    </section>
  );
}
