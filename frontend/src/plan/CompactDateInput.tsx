import { CalendarDays } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { type Strings } from "../i18n";
import {
  EDITOR_DATE_MONTHS,
  type EditorDateParts,
  formatEditorDate,
  parseEditorDate,
} from "./editorDates";

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
