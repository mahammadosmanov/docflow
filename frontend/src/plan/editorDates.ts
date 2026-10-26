/** Date parts for the plan editor's day/month/year cells. */
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
