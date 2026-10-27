import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { hasNewWarnings } from "../plan/revisionWarnings";
import { type OperationResult, type Plan, type ProgressEvent } from "../types";

export type RunStage = "form" | "previewing" | "preview" | "applying" | "result";

/** Screen state the apply run writes to. */
type ApplyScreenState = {
  setError: (message: string) => void;
  setPlan: (plan: Plan) => void;
  setEditorRows: (rows: Record<string, unknown>[]) => void;
  setEditorDirty: (dirty: boolean) => void;
  setStage: (stage: RunStage) => void;
  setResult: (result: OperationResult) => void;
  setCancelling: (cancelling: boolean) => void;
};

/** Applies the reviewed plan, first revising it when the editor has unsaved rows. */
export function useApplyRun({
  bridge,
  operationId,
  plan,
  editorDirty,
  editorRows,
  s,
  screen,
  progress,
  updateScanPreview,
}: {
  bridge: Bridge;
  operationId: string;
  plan: Plan | null;
  editorDirty: boolean;
  editorRows: Record<string, unknown>[];
  s: Strings;
  screen: ApplyScreenState;
  progress: {
    startProgress: (event: ProgressEvent) => void;
    receiveProgress: (event: ProgressEvent) => void;
  };
  updateScanPreview: () => Promise<void>;
}) {
  const { setError, setPlan, setEditorRows, setEditorDirty, setStage, setResult, setCancelling } = screen;

  const apply = async () => {
    if (editorDirty && plan?.opId === "rfi.process_scans_fast_covers") {
      await updateScanPreview();
      return;
    }
    if (!plan) return;
    setError("");
    const applying = { phase: "apply", done: 0, total: 0 };
    progress.startProgress(applying);
    try {
      let approvedPlan = plan;
      if (editorDirty) {
        const revised = await bridge.call("engine/revise", {
          planId: plan.planId,
          rows: editorRows,
        });
        setPlan(revised);
        const revisedEditor = revised.meta?.editor as
          Record<string, unknown> | undefined;
        const revisedRows = Array.isArray(revisedEditor?.rows)
          ? (revisedEditor.rows as Record<string, unknown>[])
          : editorRows;
        setEditorRows(revisedRows.map((row) => ({ ...row })));
        setEditorDirty(false);
        approvedPlan = revised;
        if (hasNewWarnings(plan, revised)) {
          setError(s.t("rfi.editor.reviewWarnings"));
          setStage("preview");
          return;
        }
      }
      setStage("applying");
      setResult(
        await bridge.call(
          "engine/apply",
          { opId: operationId, planId: approvedPlan.planId },
          progress.receiveProgress,
        ),
      );
      setCancelling(false);
      setStage("result");
    } catch (reason) {
      setCancelling(false);
      setError(humanError(reason, s));
      setStage("preview");
    }
  };

  return { apply };
}
