import {
  ArrowRight,
  Check,
  CheckCircle2,
  CircleAlert,
  Copy,
  Database,
  Files,
  FolderOpen,
  History,
  LoaderCircle,
  Mail,
  Printer,
  RotateCcw,
  TableProperties,
  Trash2,
  Undo2,
} from "lucide-react";
import {
  Button,
  Notice,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type OperationResult,
  type Step,
} from "../types";
import {
  formatConfidencePercent,
} from "../plan/EditablePlanTable";

export function outlookReplyPreview(value: unknown): {
  subject: string;
  plainText: string;
  html: string;
} | null {
  if (!value || typeof value !== "object") return null;
  const preview = value as Record<string, unknown>;
  if (
    typeof preview.subject !== "string" ||
    typeof preview.plainText !== "string" ||
    typeof preview.html !== "string"
  ) {
    return null;
  }
  return {
    subject: preview.subject,
    plainText: preview.plainText,
    html: preview.html,
  };
}

export function StepRow({
  step,
  s,
  fullPath = false,
}: {
  step: Step;
  s: Strings;
  fullPath?: boolean;
}) {
  const source = fullPath ? step.src : step.src?.split(/[\\/]/).pop();
  const destination = fullPath ? step.dst : step.dst?.split(/[\\/]/).pop();
  const confidence = formatConfidencePercent(step.meta.confidence);
  const rotationSummary =
    (step.kind === "pdf_page_rotate" || step.kind === "pdf_pages_extract") &&
    Array.isArray(step.meta.rotations)
      ? step.meta.rotations
          .map((entry) => {
            if (!entry || typeof entry !== "object") return "";
            const page = Number((entry as Record<string, unknown>).pageIndex);
            const target = Number(
              (entry as Record<string, unknown>).targetRotation,
            );
            const pageConfidence = formatConfidencePercent(
              (entry as Record<string, unknown>).confidence,
              "percent",
            );
            return Number.isInteger(page) && Number.isInteger(target)
              ? `${page + 1} → ${target}°${pageConfidence ? ` · ${s.t("ocr.confidence.short")} ${pageConfidence}` : ""}`
              : "";
          })
          .filter(Boolean)
          .join(", ")
      : "";
  const extractionPages =
    step.kind === "pdf_pages_extract" && Array.isArray(step.meta.pageIndexes)
      ? step.meta.pageIndexes
          .map((page) => Number(page))
          .filter(Number.isInteger)
          .map((page) => page + 1)
          .join(", ")
      : "";
  return (
    <div className="step-row">
      <span className="step-kind">
        {step.kind === "print" ? (
          <Printer />
        ) : step.kind === "workbook_update" ? (
          <TableProperties />
        ) : step.kind === "replica_publish" ? (
          <Database />
        ) : step.kind === "pdf_first_page_replace" ? (
          <Files />
        ) : step.kind === "pdf_page_rotate" ? (
          <RotateCcw />
        ) : step.kind === "pdf_pages_extract" ? (
          <Files />
        ) : step.kind === "cleanup_temp" ? (
          <Trash2 />
        ) : step.kind === "copy" ? (
          <Copy />
        ) : step.kind === "mkdir" ? (
          <FolderOpen />
        ) : (
          <ArrowRight />
        )}
      </span>
      <div className="step-copy">
        <div className="step-title">
          <strong>{step.noteKey ? s.t(step.noteKey) : step.kind}</strong>
          {confidence && (
            <span
              className="confidence-badge"
              title={s.t("ocr.confidence.help")}
            >
              {s.t("ocr.confidence.short")} {confidence}
            </span>
          )}
        </div>
        <span title={step.src ?? ""}>{source || "—"}</span>
        {step.dst && (
          <span className="destination" title={step.dst}>
            → {destination}
          </span>
        )}
        {rotationSummary && (
          <span>
            {s.t("rfi.orientation.pages")}: {rotationSummary}
          </span>
        )}
        {extractionPages && (
          <span>
            {s.t("rfi.extraction.pages")}: {extractionPages}
          </span>
        )}
      </div>
    </div>
  );
}

export function ResultView({
  result,
  s,
  error,
  onDone,
  onUndo,
  copySheets = [],
  copyEnabled = false,
  copyingSheet = null,
  onCopy,
  onResumePrint,
  onPrepareRfiReply,
  onOpenDestination,
}: {
  result: OperationResult;
  s: Strings;
  error?: string;
  onDone: () => void;
  onUndo: () => void;
  copySheets?: string[];
  copyEnabled?: boolean;
  copyingSheet?: string | null;
  onCopy?: (sheet?: string) => void;
  onResumePrint?: () => void;
  onPrepareRfiReply?: () => void;
  onOpenDestination?: () => void;
}) {
  const effectState =
    typeof result.meta?.effectState === "string"
      ? result.meta.effectState
      : result.meta?.resumeAvailable === true
        ? "partial_committed"
        : result.failed > 0
          ? result.applied > 0
            ? "partial_committed"
            : "no_effect_retryable"
          : "committed";
  const success =
    effectState === "committed" && result.meta?.resumeAvailable !== true;
  return (
    <div className="result-card">
      <span className={`result-icon ${success ? "success" : "failure"}`}>
        {success ? <CheckCircle2 /> : <CircleAlert />}
      </span>
      <p className="eyebrow">{success ? "DOCFLOW" : s.t("ui.warnings")}</p>
      <h1>{s.t("result.applied", { count: result.applied })}</h1>
      <p className="muted">{s.t("result.failed", { count: result.failed })}</p>
      <span className="status-label">{s.t(`effect.${effectState}`)}</span>
      {result.cloudSynced !== undefined && (
        <span className="safe-badge">
          {result.cloudSynced ? <Check /> : <History />}
          {result.cloudSynced ? s.t("history.cloud") : s.t("history.local")}
        </span>
      )}
      {result.errors.length > 0 && (
        <Notice kind="warning">
          {result.errors
            .map((item) => `${item.subject} ${s.t(item.key)}`)
            .join("\n")}
        </Notice>
      )}
      {result.meta?.resumeAvailable === true && (
        <div className="queue-result-summary">
          <strong>{s.t("print.resume.incomplete")}</strong>
          <span>
            {s.t("print.resume.summary", {
              count: Number(result.meta.resumable ?? 0),
            })}
          </span>
        </div>
      )}
      {error && <Notice kind="error">{error}</Notice>}
      {onCopy && (copySheets.length > 0 || copyEnabled) && (
        <div className="copy-actions">
          {copySheets.map((sheet) => (
            <Button
              key={sheet}
              variant="secondary"
              icon={
                copyingSheet === sheet ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <TableProperties />
                )
              }
              disabled={copyingSheet !== null}
              onClick={() => onCopy(sheet)}
            >
              {sheet} · {s.t("rfi.action.copySheet")}
            </Button>
          ))}
          {copySheets.length === 0 && copyEnabled && (
            <Button
              variant="secondary"
              icon={
                copyingSheet !== null ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <TableProperties />
                )
              }
              disabled={copyingSheet !== null}
              onClick={() => onCopy()}
            >
              {s.t("rfi.action.copyMail")}
            </Button>
          )}
        </div>
      )}
      <div className="result-actions">
        {onPrepareRfiReply && (
          <Button
            variant="secondary"
            icon={<Mail />}
            onClick={onPrepareRfiReply}
          >
            {s.t("rfi.action.prepareReply")}
          </Button>
        )}
        {onResumePrint && (
          <Button
            variant="secondary"
            icon={<RotateCcw />}
            onClick={onResumePrint}
          >
            {s.t("print.resume.review")}
          </Button>
        )}
        {onOpenDestination && (
          <Button
            variant="secondary"
            icon={<FolderOpen />}
            onClick={onOpenDestination}
          >
            {s.t("action.openDestination")}
          </Button>
        )}
        <Button
          variant="secondary"
          icon={<Undo2 />}
          disabled={!result.journalId || result.meta?.nonUndoable === true}
          onClick={onUndo}
        >
          {s.t("action.undo")}
        </Button>
        <Button icon={<Check />} onClick={onDone}>
          {s.t("action.done")}
        </Button>
      </div>
    </div>
  );
}
