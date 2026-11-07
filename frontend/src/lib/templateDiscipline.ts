/** The four RFI disciplines a template can belong to. */
export const TEMPLATE_DISCIPLINES = ["ELE", "MEC", "CIV", "PRC"] as const;

const PREFIX = /^(?:ELE|MEC|CIV|PRC)-/i;

/** Starts `value` with `ELE-` style text for the discipline, replacing another discipline's prefix. */
export function withDisciplinePrefix(value: string, discipline: string): string {
  const rest = value.replace(PREFIX, "");
  return discipline ? `${discipline}-${rest}` : rest;
}
