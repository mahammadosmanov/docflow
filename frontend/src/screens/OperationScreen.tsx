import {
  Eye,
  FolderOpen,
  RotateCcw,
  SlidersHorizontal,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
} from "react";
import {
  ActionBar,
  Button,
  Header,
  Notice,
  StepIndicator,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  chooseFolder,
  revealPath,
} from "../lib/platform";
import {
  type OperationDef,
  type OperationResult,
  type PathSelection,
  type Plan,
} from "../types";
import {
  isPathSelection,
  parseRfiInputs,
} from "../helpers";
import {
  PresetToolbar,
} from "./PresetToolbar";
import {
  SchemaField,
} from "./SchemaField";
import {
  copySheetsFromPlan,
} from "../plan/EditablePlanTable";
import {
  PlanView,
} from "../plan/PlanView";
import {
  ResultView,
} from "./ResultView";
import { BusyStage } from "./BusyStage";
import { PrintQueueResumeCard } from "./PrintQueueResumeCard";
import { RfiPrintFinderPanel } from "./RfiPrintFinderPanel";
import { useRfiPrintFinder } from "./useRfiPrintFinder";
import { editorRowsOf } from "./operationState";
import { useProgressTrail } from "./useProgressTrail";
import { usePrintQueue } from "./usePrintQueue";
import { useScanPreviewUpdate } from "./useScanPreviewUpdate";
import { useRunCancel } from "./useRunCancel";
import { useCopyTable } from "./useCopyTable";
import { useResultActions } from "./useResultActions";
import { useSavedParams } from "./useSavedParams";
import { useApplyRun } from "./useApplyRun";
import { useOperationShortcuts } from "./useOperationShortcuts";
import { usePreviewRun } from "./usePreviewRun";

export function OperationScreen({
  bridge,
  operation,
  initialValues,
  initialPlan,
  s,
  onBack,
  onReview,
  onPrepareRfiReply,
}: {
  bridge: Bridge;
  operation: OperationDef;
  initialValues?: Record<string, unknown>;
  initialPlan?: Plan;
  s: Strings;
  onBack: () => void;
  onReview: (destination: PathSelection) => void;
  onPrepareRfiReply: (plan: Plan) => void;
}) {
  const defaults = Object.fromEntries(
    operation.schema.map((item) => [item.name, item.default]),
  );
  const [values, setValues] = useState<Record<string, unknown>>({
    ...defaults,
    ...(initialValues ?? {}),
  });
  const [stage, setStage] = useState<
    "form" | "previewing" | "preview" | "applying" | "result"
  >(initialPlan ? "preview" : "form");
  const [plan, setPlan] = useState<Plan | null>(initialPlan ?? null);
  const [result, setResult] = useState<OperationResult | null>(null);
  const { progress, progressTrail, receiveProgress, startProgress } =
    useProgressTrail();
  const [error, setError] = useState("");
  const [invalidField, setInvalidField] = useState<string | null>(null);
  const [editorRows, setEditorRows] = useState<Record<string, unknown>[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  const rfiInputs = operation.id === "files.batch_print" ? parseRfiInputs(values.rfi_numbers) : [];
  const rfiFinder = useRfiPrintFinder({ bridge, s, values, rfiInputs });
  const { invalidateRfiFinder, setExcludedRfiInputs } = rfiFinder;
  const [scanPreviewUpdated, setScanPreviewUpdated] = useState(false);
  const {
    valuesRevision,
    lastSaved,
    previousValues,
    setPreviousValues,
    saveError,
  } = useSavedParams({
      bridge,
      s,
      operation,
      initialValues,
      values,
      defaults,
      setValues,
    });


  const installPlan = (next: Plan) => {
    setPlan(next);
    setEditorRows(editorRowsOf(next).map((row) => ({ ...row })));
    setEditorDirty(false);
    setScanPreviewUpdated(false);
    setStage("preview");
  };

  const { preview } = usePreviewRun({
    bridge,
    operation,
    values,
    lastSaved,
    finder: { ran: rfiFinder.rfiFinderWasRun, candidates: rfiFinder.rfiPrintCandidates },
    s,
    screen: { setError, setStage, setPreviousValues, installPlan, setInvalidField },
    progress: { startProgress, receiveProgress },
  });
  const stageFocusRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (stage === "preview" || stage === "result") {
      stageFocusRef.current?.focus();
    }
  }, [stage]);
  const { cancelling, setCancelling, cancelPreview, cancelApply } = useRunCancel({
    bridge,
    operationId: operation.id,
    plan,
    s,
    setError,
    onPreviewCancelled: () => setStage("form"),
  });
  const { latestPrintQueue, resumePrintQueue, discardPrintQueue } =
    usePrintQueue({
      bridge,
      operationId: operation.id,
      s,
      setError,
      setStage,
      installPlan,
    });
  const { isUpdatingScanPreview, updateScanPreview } = useScanPreviewUpdate({
    bridge,
    plan,
    editorDirty,
    editorRows,
    s,
    setError,
    onRevised: (revised) => {
      installPlan(revised);
      setScanPreviewUpdated(true);
    },
  });
  const { apply } = useApplyRun({
    bridge,
    operationId: operation.id,
    plan,
    editorDirty,
    editorRows,
    s,
    screen: {
      setError,
      setPlan,
      setEditorRows,
      setEditorDirty,
      setStage,
      setResult,
      setCancelling,
    },
    progress: { startProgress, receiveProgress },
    updateScanPreview,
  });
  const { copyingSheet, copyTable } = useCopyTable({ bridge, plan, s, setError });
  const { prepareRfiReply, undo } = useResultActions({
    bridge,
    plan,
    result,
    s,
    setError,
    onBack,
    onPrepareRfiReply,
  });

  useOperationShortcuts({
    stage,
    cancelling,
    preview,
    apply,
    cancelPreview,
    setStage,
    onBack,
  });

  const currentStep =
    stage === "form"
      ? 0
      : stage === "previewing" || stage === "preview"
        ? 1
        : 2;

  return (
    <div className="screen">
      <Header
        title={s.t(operation.titleKey)}
        titleAs="h1"
        onBack={onBack}
        backLabel={s.t("action.back")}
      >
        <StepIndicator
          current={currentStep}
          labels={[s.t("step.form"), s.t("step.preview"), s.t("step.apply")]}
        />
      </Header>
      <main
        className={`work-pane ${
          stage === "preview" && plan?.meta?.editor ? "editor-pane" : ""
        }`}
      >
        {stage === "form" && (
          <div className="flow-layout">
            <div className="flow-scroll">
              <section className="content-card">
                <div className="card-title">
                  <SlidersHorizontal />
                  <div>
                    <h2>{s.t("step.form")}</h2>
                    <p>{s.t(operation.descriptionKey)}</p>
                  </div>
                </div>
                {operation.supportsPresets && (
                  <PresetToolbar
                    bridge={bridge}
                    operation={operation}
                    values={values}
                    s={s}
                    onApply={(preset) => {
                      valuesRevision.current += 1;
                      setValues((current) => ({
                        ...current,
                        ...preset.values,
                      }));
                    }}
                    onError={setError}
                  />
                )}
                {operation.supportsPresets && previousValues && (
                  <div className="resume-queue-card">
                    <div>
                      <strong>{s.t("params.previous.title")}</strong>
                      <small>{s.t("params.previous.help")}</small>
                    </div>
                    <Button
                      variant="secondary"
                      icon={<RotateCcw />}
                      onClick={() => {
                        valuesRevision.current += 1;
                        setValues((current) => ({
                          ...current,
                          ...previousValues,
                        }));
                      }}
                    >
                      {s.t("params.previous.apply")}
                    </Button>
                  </div>
                )}
                <div className="form-stack schema-form">
                  {operation.schema
                    .filter((param) =>
                      Object.entries(param.visibleWhen ?? {}).every(
                        ([name, expected]) => values[name] === expected,
                      ),
                    )
                    .map((param) => (
                      <SchemaField
                        key={param.name}
                        bridge={bridge}
                        operationId={operation.id}
                        param={param}
                        invalid={invalidField === param.name}
                        value={values[param.name]}
                        values={values}
                        s={s}
                        onChange={(value) => {
                          valuesRevision.current += 1;
                          if (invalidField === param.name) setInvalidField(null);
                          setValues((current) => ({
                            ...current,
                            [param.name]: value,
                          }));
                          if (
                            operation.id === "files.batch_print" &&
                            ["rfi_system_root", "rfi_numbers", "include_first_page", "include_sketch"].includes(param.name)
                          ) {
                            invalidateRfiFinder();
                          }
                          if (param.name === "rfi_numbers") setExcludedRfiInputs([]);
                        }}
                      />
                    ))}
                </div>
                {operation.id === "files.batch_print" && (
                  <RfiPrintFinderPanel s={s} inputs={rfiInputs} finder={rfiFinder} />
                )}
                {latestPrintQueue && (
                  <PrintQueueResumeCard
                    queue={latestPrintQueue}
                    s={s}
                    onDiscard={(queueId) => discardPrintQueue(queueId)}
                    onResume={(queueId) => resumePrintQueue(queueId)}
                  />
                )}
              </section>
              {error && <Notice kind="error">{error}</Notice>}
              {saveError && <Notice kind="error">{saveError}</Notice>}
            </div>
            <ActionBar
              left={
                operation.schema.some((param) => param.name === "dest") ? (
                  <Button
                    variant="ghost"
                    icon={<FolderOpen />}
                    onClick={() =>
                      void chooseFolder(
                        bridge,
                        "rfi.review",
                        "destination",
                      ).then((destination) => {
                        if (destination) onReview(destination);
                      })
                    }
                  >
                    {s.t("action.review")}
                  </Button>
                ) : undefined
              }
              right={
                <Button icon={<Eye />} onClick={() => void preview()}>
                  {s.t("action.preview")}
                </Button>
              }
            />
          </div>
        )}
        {stage === "previewing" && (
          <BusyStage
            label={s.t("action.preview")}
            progress={progress}
            progressTrail={progressTrail}
            s={s}
            cancelling={cancelling}
            cancelLabel={s.t(cancelling ? "operation.cancelling" : "action.cancel")}
            onCancel={() => void cancelPreview()}
          />
        )}
        {stage === "preview" && plan && (
          <PlanView
            focusRef={stageFocusRef}
            plan={plan}
            s={s}
            error={error}
            onCancel={() => setStage("form")}
            onApply={() => void apply()}
            editorRows={editorRows}
            onEditorRowsChange={(rows) => {
              setEditorRows(rows);
              setEditorDirty(true);
            }}
            editorDirty={editorDirty}
            onUpdatePreview={() => void updateScanPreview()}
            isUpdatingPreview={isUpdatingScanPreview}
            scanPreviewUpdated={scanPreviewUpdated}
          />
        )}
        {stage === "applying" && (
          <BusyStage
            label={s.t("plan.applying")}
            progress={progress}
            progressTrail={progressTrail}
            s={s}
            cancelling={cancelling}
            cancelLabel={s.t(
              cancelling
                ? "operation.cancelling"
                : operation.id === "files.batch_print"
                  ? "print.cancelQueue"
                  : "operation.cancel",
            )}
            onCancel={() => void cancelApply()}
          />
        )}
        {stage === "result" && result && (
          <ResultView
            result={result}
            focusRef={stageFocusRef}
            s={s}
            error={error}
            onDone={onBack}
            onUndo={() => void undo()}
            onResumePrint={
              operation.id === "files.batch_print" &&
              result.meta?.resumeAvailable === true
                ? () =>
                    void resumePrintQueue(String(result.meta?.queueId ?? ""))
                : undefined
            }
            copySheets={copySheetsFromPlan(plan)}
            copyEnabled={Boolean(
              (plan?.meta?.editor as Record<string, unknown> | undefined)
                ?.copyEnabled,
            )}
            copyingSheet={copyingSheet}
            onCopy={(sheet) => void copyTable(sheet)}
            onPrepareRfiReply={
              operation.id === "rfi.log_add" &&
              result.applied > 0 &&
              result.failed === 0 &&
              result.meta?.resumeAvailable !== true
                ? () => void prepareRfiReply()
                : undefined
            }
            onOpenDestination={destinationRevealAction(bridge, values)}
          />
        )}
      </main>
    </div>
  );
}

function destinationRevealAction(bridge: Bridge, values: Record<string, unknown>) {
  const dest =
    (isPathSelection(values.destination) && values.destination) ||
    (isPathSelection(values.dest) && values.dest) ||
    (isPathSelection(values.target) && values.target) ||
    (isPathSelection(values.output_directory) && values.output_directory) ||
    (isPathSelection(values.rfi_system) && values.rfi_system);
  return dest ? () => void revealPath(bridge, dest.selectionId) : undefined;
}
