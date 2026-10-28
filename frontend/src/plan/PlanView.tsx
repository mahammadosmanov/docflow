import {
  Eye,
  FileCheck2,
  Play,
  RefreshCw,
} from "lucide-react";
import {
  type RefObject,
  useState,
} from "react";
import {
  Button,
} from "../components/ui";
import {
  EmptyState,
  Notice,
} from "../components/feedback";
import { ActionBar } from "../components/ActionBar";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";
import {
  EditablePlanTable,
  isPlanEditorReady,
} from "./EditablePlanTable";
import {
  OutlookReplyPreview,
} from "./OutlookReplyPreview";
import {
  PlanExactEffects,
  PlanImpactSummary,
  ScanIndexSummary,
} from "./PlanSummaries";
import {
  PlanWarnings,
} from "./PlanWarnings";
import {
  SignedStageNavigator,
  SignedWarningReviewSummary,
} from "./PlanSignedSummaries";
import {
  WorkbookChangePreview,
} from "./WorkbookChangePreview";
import {
  SignedPageComparison,
} from "./SignedPageComparison";
import {
  StepRow,
  outlookReplyPreview,
} from "../screens/ResultView";

export function PlanView({
  plan,
  s,
  error,
  onCancel,
  onApply,
  editorRows,
  onEditorRowsChange,
  editorDirty = false,
  onUpdatePreview,
  isUpdatingPreview = false,
  scanPreviewUpdated = false,
  focusRef,
}: {
  plan: Plan;
  s: Strings;
  error: string;
  onCancel: () => void;
  onApply: () => void;
  editorRows?: Record<string, unknown>[];
  onEditorRowsChange?: (rows: Record<string, unknown>[]) => void;
  editorDirty?: boolean;
  onUpdatePreview?: () => void;
  isUpdatingPreview?: boolean;
  scanPreviewUpdated?: boolean;
  focusRef?: RefObject<HTMLDivElement | null>;
}) {
  const [warningFocusCode, setWarningFocusCode] = useState("");
  const moves = plan.steps.filter((step) => step.kind !== "mkdir").length;
  const editor =
    plan.meta?.editor && typeof plan.meta.editor === "object"
      ? (plan.meta.editor as Record<string, unknown>)
      : null;
  const editorReady =
    !editor || !editorRows || isPlanEditorReady(editor, editorRows);
  const editorCanMaterializeStep = Boolean(
    editorRows?.some((row) => row.enabled !== false) &&
    ((Array.isArray(editor?.requiredFields) &&
      editor.requiredFields.length > 0) ||
      (plan.opId === "files.batch_print" && editor?.kind === "recordTable")),
  );
  const canApply =
    editorReady && (plan.steps.length > 0 || editorCanMaterializeStep);
  const isScanEditor = editor?.kind === "rfiScan";
  const shouldUpdatePreview = isScanEditor && editorDirty;
  const replyPreview = outlookReplyPreview(plan.meta?.replyPreview);
  const focusableWarningCodes =
    plan.opId === "rfi.stage_signed_first_pages"
      ? (editorRows ?? [])
          .map((row) => (typeof row.code === "string" ? row.code : ""))
          .filter(Boolean)
      : [];
  const signedStage = plan.opId === "rfi.stage_signed_first_pages";
  return (
    <div className="flow-layout" ref={focusRef} tabIndex={-1}>
      <div className="flow-scroll plan-flow-scroll">
        <PlanImpactSummary plan={plan} s={s} />
        {scanPreviewUpdated && !editorDirty && (
          <div role="status" aria-live="polite">
            <Notice kind="info">
              {s.t("rfi.fastScan.editor.previewUpdated")}
            </Notice>
          </div>
        )}
        <ScanIndexSummary plan={plan} s={s} />
        {editor && editorRows && onEditorRowsChange ? (
          <>
            {signedStage && (
              <SignedStageNavigator plan={plan} rows={editorRows} s={s} />
            )}
            {!signedStage && plan.warnings.length > 0 && (
              <PlanWarnings warnings={plan.warnings} focusableCodes={[]} s={s} />
            )}
            <WorkbookChangePreview plan={plan} s={s} />
            {replyPreview && <OutlookReplyPreview preview={replyPreview} s={s} />}
            {signedStage ? (
              <>
                <div id="signed-approved-records">
                  <EditablePlanTable
                    editor={editor}
                    rows={editorRows}
                    s={s}
                    onChange={onEditorRowsChange}
                  />
                </div>
                <SignedPageComparison
                  plan={plan}
                  editorRows={editorRows}
                  focusCode={warningFocusCode}
                  s={s}
                />
                <SignedWarningReviewSummary plan={plan} s={s} />
                {plan.warnings.length > 0 && (
                  <PlanWarnings
                    warnings={plan.warnings}
                    focusableCodes={focusableWarningCodes}
                    onFocusCode={setWarningFocusCode}
                    s={s}
                  />
                )}
              </>
            ) : (
              <EditablePlanTable
                editor={editor}
                rows={editorRows}
                s={s}
                onChange={onEditorRowsChange}
              />
            )}
            {plan.steps.length > 0 && <PlanExactEffects plan={plan} s={s} />}
          </>
        ) : (
          <>
            {plan.warnings.length > 0 && (
              <PlanWarnings
                warnings={plan.warnings}
                focusableCodes={[]}
                s={s}
              />
            )}
            <WorkbookChangePreview plan={plan} s={s} />
            {replyPreview && <OutlookReplyPreview preview={replyPreview} s={s} />}
            <section className="plan-section">
              <div className="plan-heading">
                <strong>{s.t("plan.summary", { count: moves })}</strong>
                <span>
                  <Eye /> {s.t("plan.readOnly")}
                </span>
              </div>
              {plan.steps.length ? (
                <div className="step-list">
                  {plan.steps.map((step, index) => (
                    <StepRow key={`${step.kind}-${index}`} step={step} s={s} />
                  ))}
                </div>
              ) : (
                <EmptyState icon={<FileCheck2 />} title={s.t("plan.empty")} />
              )}
            </section>
          </>
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </div>
      <ActionBar
        left={
          <Button
            variant="ghost"
            disabled={isUpdatingPreview}
            onClick={onCancel}
          >
            {s.t("action.cancel")}
          </Button>
        }
        right={
          <Button
            icon={shouldUpdatePreview ? <RefreshCw /> : <Play />}
            disabled={!canApply || isUpdatingPreview}
            onClick={shouldUpdatePreview ? (onUpdatePreview ?? onApply) : onApply}
          >
            {isUpdatingPreview
              ? s.t("rfi.fastScan.editor.updatingPreview")
              : shouldUpdatePreview
                ? s.t("rfi.fastScan.editor.updatePreview")
                : s.t("action.apply")}
          </Button>
        }
      />
    </div>
  );
}
