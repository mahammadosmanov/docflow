import type { ParamDef } from "../types";

/**
 * Keeps the values a procedure may remember between sessions: skips parameters
 * marked `remember: false` and never stores long text or secrets. Names that the
 * schema does not declare are kept, matching the form's existing behavior.
 */
export function persistableValues(
  schema: ParamDef[],
  values: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(values).filter(([name]) => {
      const param = schema.find((item) => item.name === name);
      return (
        param?.remember !== false &&
        param?.kind !== "longText" &&
        param?.kind !== "secret"
      );
    }),
  );
}
