import type { ProgressHandler } from "./bridge";
import { normalizeMockRfiCode } from "./mockFixtures";
import type { MockPlanState } from "./mockPlanHandlers";
import type { Plan } from "../types";

export function reviseOperation(
  state: MockPlanState,
  params: Record<string, unknown>,
  onProgress?: ProgressHandler,
): unknown {
  let result: unknown;
  if (state.fastScanPlan && params.planId === state.fastScanPlan.planId) {
    const editor = state.fastScanPlan.meta.editor as Record<
      string,
      unknown
    >;
    const originalRows = Array.isArray(editor.rows)
      ? (editor.rows as Record<string, unknown>[])
      : [];
    const submittedRows = Array.isArray(params.rows)
      ? (params.rows as Record<string, unknown>[])
      : [];
    const edit = state.fastScanPlan.meta.scanEdit as Record<
      string,
      unknown
    >;
    const destination = String(edit.destinationRoot ?? "");
    const organize = edit.organizeIntoRfiFolders === true;
    const seenCodes = new Set<string>();
    if (submittedRows.length !== originalRows.length) {
      throw new Error("error.rfi.fastScan.revisionInvalid");
    }
    const rows = submittedRows.map<Record<string, unknown>>(
      (submitted, index) => {
        const original = originalRows[index];
        const inputCode = submitted.code;
        const expected = { ...original, code: inputCode };
        if (JSON.stringify(submitted) !== JSON.stringify(expected)) {
          throw new Error("error.rfi.fastScan.revisionInvalid");
        }
        const code = normalizeMockRfiCode(inputCode);
        if (!code) {
          throw new Error("error.rfi.fastScan.codeInvalid");
        }
        if (seenCodes.has(code)) {
          throw new Error("error.rfi.fastScan.targetExists");
        }
        seenCodes.add(code);
        const discipline = code.split("-")[5];
        const target = `${destination}${
          organize ? `/${discipline}` : ""
        }/RFI No. ${code}.pdf`;
        return { ...expected, code, target };
      },
    );
    const steps = state.fastScanPlan.steps.map((step) => {
      const rowId = step.meta.scanEditRowId;
      const row = rows.find((candidate) => candidate._rowId === rowId);
      return row
        ? {
            ...step,
            dst: String(row.target),
            meta: { ...step.meta, code: row.code },
          }
        : step;
    });
    const revised = {
      ...state.fastScanPlan,
      planId: `${state.fastScanPlan.planId}-revised`,
      meta: {
        ...state.fastScanPlan.meta,
        editor: { ...editor, rows },
      },
      steps,
    } satisfies Plan;
    state.fastScanPlan = revised;
    result = revised;
    return result;
  }
  if (
    state.completeStatusPlan &&
    params.planId === state.completeStatusPlan.planId
  ) {
    const rows = Array.isArray(params.rows)
      ? params.rows.map((row) => ({ ...row }))
      : [];
    const editor = state.completeStatusPlan.meta.editor as Record<
      string,
      unknown
    >;
    const revised = {
      ...state.completeStatusPlan,
      planId: "demo-complete-status-plan-revised",
      meta: {
        ...state.completeStatusPlan.meta,
        editor: { ...editor, rows },
      },
      steps: rows.some((row) => row.enabled !== false)
        ? state.completeStatusPlan.steps
        : [],
    } satisfies Plan;
    state.completeStatusPlan = revised;
    result = revised;
    return result;
  }
  if (state.fileGroupPlan && params.planId === state.fileGroupPlan.planId) {
    const rows = Array.isArray(params.rows)
      ? params.rows.map((row) => ({ ...row }))
      : [];
    const editor = state.fileGroupPlan.meta.editor as Record<string, unknown>;
    const enabledRows = rows.filter((row) => row.enabled !== false);
    const revised = {
      ...state.fileGroupPlan,
      planId: "demo-file-group-plan-revised",
      meta: {
        ...state.fileGroupPlan.meta,
        editor: { ...editor, rows },
      },
      steps: state.fileGroupPlan.steps.filter((step) => {
        const rowId = String(step.meta.rowId ?? "");
        if (rowId) return enabledRows.some((row) => row._rowId === rowId);
        const code = String(step.meta.code ?? "");
        return !code || enabledRows.some((row) => row.rfiCode === code);
      }),
    } satisfies Plan;
    state.fileGroupPlan = revised;
    result = revised;
    return result;
  }
  if (!state.printPlan || params.planId !== state.printPlan.planId) {
    throw new Error("Unknown demo print plan");
  }
  const rows = Array.isArray(params.rows)
    ? params.rows.map((row) => ({ ...row }))
    : [];
  const editor = state.printPlan.meta.editor as Record<string, unknown>;
  const revised = {
    ...state.printPlan,
    planId: "demo-print-plan-revised",
    meta: {
      ...state.printPlan.meta,
      editor: { ...editor, rows },
    },
    steps: rows
      .filter((row) => row.enabled !== false)
      .map((row) => ({
        kind: "print" as const,
        src: String(row.path),
        dst: String(row.path),
        noteKey: "note.print.queued",
        meta: {},
      })),
  } satisfies Plan;
  state.printPlan = revised;
  result = revised;
  return result;
}
