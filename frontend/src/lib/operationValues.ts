import type { ParamDef } from "../types";

/**
 * True when every `visibleWhen` condition of the parameter matches the current
 * form values. Parameters without conditions are always visible.
 */
export function isParamVisible(
  param: ParamDef,
  values: Record<string, unknown>,
): boolean {
  return Object.entries(param.visibleWhen ?? {}).every(
    ([name, expected]) => values[name] === expected,
  );
}

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

/** The values sent to `engine/preview`. Batch print takes its file list from the
 * RFI finder when the finder ran, and drops finder-only inputs. */
export function previewValuesFor(
  operationId: string,
  values: Record<string, unknown>,
  finder: {
    ran: boolean;
    candidates: { selected: boolean; selection: unknown }[];
  },
): Record<string, unknown> {
  const previewValues = { ...values };
  if (operationId !== "files.batch_print") return previewValues;
  delete previewValues.rfi_system_root;
  delete previewValues.rfi_numbers;
  delete previewValues.include_first_page;
  delete previewValues.include_sketch;
  if (finder.ran) {
    previewValues.files = finder.candidates
      .filter((candidate) => candidate.selected)
      .map((candidate) => candidate.selection);
  }
  return previewValues;
}
