import { type Plan, type Warning } from "../types";

const warningKey = (warning: Warning) => `${warning.key}\u0000${warning.subject}`;

/** True when a revised plan carries a warning the previous plan did not have. */
export function hasNewWarnings(previous: Plan, revised: Plan): boolean {
  const known = new Set(previous.warnings.map(warningKey));
  return revised.warnings.some((warning) => !known.has(warningKey(warning)));
}
