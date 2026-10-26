import {
  Eye,
  FolderOpen,
  RotateCcw,
  SlidersHorizontal,
} from "lucide-react";
import {
  useCallback,
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
  type PrintQueueSummary,
  type ProgressEvent,
  type RfiPrintCandidate,
  type RfiPrintResolution,
} from "../types";
import {
  humanError,
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
import { persistableValues, previewValuesFor } from "../lib/operationValues";

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
  const valuesRevision = useRef(0);
  const [hydrated, setHydrated] = useState(false);
  const lastSaved = useRef("");
  const [stage, setStage] = useState<
    "form" | "previewing" | "preview" | "applying" | "result"
  >(initialPlan ? "preview" : "form");
  const [plan, setPlan] = useState<Plan | null>(initialPlan ?? null);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [progressTrail, setProgressTrail] = useState<ProgressEvent[]>([]);
  const [error, setError] = useState("");
  const [editorRows, setEditorRows] = useState<Record<string, unknown>[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  const [rfiPrintCandidates, setRfiPrintCandidates] = useState<RfiPrintCandidate[]>([]);
  const [rfiPrintResolutions, setRfiPrintResolutions] = useState<RfiPrintResolution[]>([]);
  const [rfiFinderBusy, setRfiFinderBusy] = useState(false);
  const [rfiFinderError, setRfiFinderError] = useState("");
  const [excludedRfiInputs, setExcludedRfiInputs] = useState<string[]>([]);
  const [rfiFinderWasRun, setRfiFinderWasRun] = useState(false);
  const rfiSearchRevision = useRef(0);
  const [rfiSearchTruncated, setRfiSearchTruncated] = useState(false);
  const invalidateRfiFinder = () => {
    rfiSearchRevision.current += 1;
    setRfiFinderBusy(false);
    setRfiFinderError("");
    setRfiFinderWasRun(false);
    setRfiSearchTruncated(false);
    setRfiPrintCandidates([]);
    setRfiPrintResolutions([]);
  };
  const rfiInputs = operation.id === "files.batch_print" ? parseRfiInputs(values.rfi_numbers) : [];
  const [isUpdatingScanPreview, setIsUpdatingScanPreview] = useState(false);
  const [scanPreviewUpdated, setScanPreviewUpdated] = useState(false);
  const [copyingSheet, setCopyingSheet] = useState<string | null>(null);
  const [latestPrintQueue, setLatestPrintQueue] =
    useState<PrintQueueSummary | null>(null);
  const [previousValues, setPreviousValues] = useState<Record<
    string,
    unknown
  > | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const receiveProgress = useCallback((event: ProgressEvent) => {
    setProgress(event);
    setProgressTrail((current) => {
      const next = [...current];
      const last = next.at(-1);
      if (last?.phase === event.phase) next[next.length - 1] = event;
      else next.push(event);
      return next.slice(-8);
    });
  }, []);

  useEffect(() => {
    setHydrated(false);
    setPreviousValues(null);
    const hydrationRevision = ++valuesRevision.current;
    void bridge
      .call("settings/params", {
        opId: operation.id,
      })
      .then((saved) => {
        if (valuesRevision.current !== hydrationRevision) {
          setHydrated(true);
          return;
        }
        const known = new Set(operation.schema.map((item) => item.name));
        const filtered = Object.fromEntries(
          Object.entries(saved).filter(([key]) => {
            const parameter = operation.schema.find(
              (item) => item.name === key,
            );
            return known.has(key) && parameter?.remember !== false;
          }),
        );
        setPreviousValues(
          operation.supportsPresets && Object.keys(filtered).length > 0
            ? filtered
            : null,
        );
        setValues((current) => ({
          ...current,
          ...(operation.supportsPresets ? {} : filtered),
          ...(initialValues ?? {}),
        }));
        lastSaved.current = JSON.stringify(
          persistableValues(operation.schema, {
            ...defaults,
            ...filtered,
            ...(initialValues ?? {}),
          }),
        );
        setHydrated(true);
      })
      .catch(() => {
        setHydrated(true);
      });
  }, [bridge, operation, initialValues]);

  useEffect(() => {
    // Preset-capable procedures start from clean defaults. Their safe scalar
    // settings are captured only when Preview is requested and are restored
    // later solely through the explicit previous-settings action.
    if (!hydrated || operation.supportsPresets) return;
    const sanitized = persistableValues(operation.schema, values);
    const serialized = JSON.stringify(sanitized);
    if (serialized === lastSaved.current) return;

    const timer = setTimeout(() => {
      lastSaved.current = serialized;
      void bridge
        .call("settings/saveParams", {
          opId: operation.id,
          values: sanitized,
        })
        .catch(() => {});
    }, 200);
    return () => clearTimeout(timer);
  }, [bridge, operation, values, hydrated]);

  useEffect(() => {
    if (operation.id !== "files.batch_print") return;
    void bridge
      .call("engine/printQueueLatest")
      .then(setLatestPrintQueue)
      .catch(() => setLatestPrintQueue(null));
  }, [bridge, operation.id]);

  const installPlan = (next: Plan) => {
    setPlan(next);
    const editor = next.meta?.editor;
    const rows =
      editor &&
      typeof editor === "object" &&
      Array.isArray((editor as Record<string, unknown>).rows)
        ? ((editor as Record<string, unknown>).rows as Record<
            string,
            unknown
          >[])
        : [];
    setEditorRows(rows.map((row) => ({ ...row })));
    setEditorDirty(false);
    setScanPreviewUpdated(false);
    setStage("preview");
  };

  const searchRfiDocuments = async () => {
    setRfiFinderError("");
    setRfiFinderWasRun(false);
    setRfiPrintCandidates([]);
    setRfiPrintResolutions([]);
    setRfiSearchTruncated(false);
    const root = values.rfi_system_root;
    const requestRevision = ++rfiSearchRevision.current;
    if (!isPathSelection(root)) {
      setRfiFinderError(s.t("print.rfi.rootRequired"));
      return;
    }
    const numbers = rfiInputs.filter((input) => !excludedRfiInputs.includes(input));
    if (numbers.length === 0) {
      setRfiFinderError(s.t("print.rfi.inputRequired"));
      return;
    }
    if (values.include_first_page !== true && values.include_sketch !== true) {
      setRfiFinderError(s.t("print.rfi.typesRequired"));
      return;
    }
    setRfiFinderBusy(true);
    try {
      const result = await bridge.call("rfi/printFind", {
        root,
        numbers,
        firstPage: values.include_first_page === true,
        sketch: values.include_sketch === true,
      });
      if (requestRevision !== rfiSearchRevision.current) return;
      setRfiPrintCandidates(result.candidates);
      setRfiPrintResolutions(result.resolutions);
      setRfiFinderWasRun(true);
      setRfiSearchTruncated(result.truncated);
    } catch (reason) {
      if (requestRevision === rfiSearchRevision.current) {
        setRfiFinderError(humanError(reason, s));
      }
    } finally {
      if (requestRevision === rfiSearchRevision.current) setRfiFinderBusy(false);
    }
  };
  const setRfiCandidateGroup = (
    predicate: (candidate: RfiPrintCandidate) => boolean,
    selected: boolean,
  ) => {
    setRfiPrintCandidates((current) =>
      current.map((candidate) =>
        predicate(candidate) ? { ...candidate, selected } : candidate,
      ),
    );
  };

  const preview = async () => {
    setError("");
    const preparing = { phase: "preparing", done: 0, total: 0 };
    setProgress(preparing);
    setProgressTrail([preparing]);
    setStage("previewing");
    try {
      const sanitized = persistableValues(operation.schema, values);
      lastSaved.current = JSON.stringify(sanitized);
      await bridge.call("settings/saveParams", {
        opId: operation.id,
        values: sanitized,
      });
      if (operation.supportsPresets && Object.keys(sanitized).length > 0) {
        setPreviousValues(sanitized);
      }
      const previewValues = previewValuesFor(operation.id, values, {
        ran: rfiFinderWasRun,
        candidates: rfiPrintCandidates,
      });
      const next = await bridge.call(
        "engine/preview",
        { opId: operation.id, values: previewValues },
        receiveProgress,
      );
      installPlan(next);
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("form");
    }
  };
  const cancelPreview = async () => {
    if (cancelling) return;
    setCancelling(true);
    try {
      await bridge.call("engine/cancel", { opId: operation.id });
    } catch {
      // Ignored if preview already completed or errored
    } finally {
      setCancelling(false);
      setStage("form");
    }
  };
  const resumePrintQueue = async (queueId: string) => {
    setError("");
    setStage("previewing");
    try {
      installPlan(await bridge.call("engine/printQueueResume", { queueId }));
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("form");
    }
  };
  const discardPrintQueue = async (queueId: string) => {
    setError("");
    try {
      await bridge.call("engine/printQueueDiscard", { queueId });
      setLatestPrintQueue(null);
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const cancelApply = async () => {
    if (!plan || cancelling) return;
    setCancelling(true);
    try {
      await bridge.call("engine/cancel", { planId: plan.planId });
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const updateScanPreview = async () => {
    if (
      !plan ||
      plan.opId !== "rfi.process_scans_fast_covers" ||
      !editorDirty ||
      isUpdatingScanPreview
    ) {
      return;
    }
    setIsUpdatingScanPreview(true);
    setError("");
    try {
      const revised = await bridge.call("engine/revise", {
        planId: plan.planId,
        rows: editorRows,
      });
      installPlan(revised);
      setScanPreviewUpdated(true);
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setIsUpdatingScanPreview(false);
    }
  };
  const apply = async () => {
    if (editorDirty && plan?.opId === "rfi.process_scans_fast_covers") {
      await updateScanPreview();
      return;
    }
    if (!plan) return;
    setError("");
    const applying = { phase: "apply", done: 0, total: 0 };
    setProgress(applying);
    setProgressTrail([applying]);
    try {
      let approvedPlan = plan;
      if (editorDirty) {
        const previousWarnings = new Set(
          plan.warnings.map(
            (warning) => `${warning.key}\u0000${warning.subject}`,
          ),
        );
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
        if (
          revised.warnings.some(
            (warning) =>
              !previousWarnings.has(`${warning.key}\u0000${warning.subject}`),
          )
        ) {
          setError(s.t("rfi.editor.reviewWarnings"));
          setStage("preview");
          return;
        }
      }
      setStage("applying");
      setResult(
        await bridge.call(
          "engine/apply",
          { opId: operation.id, planId: approvedPlan.planId },
          receiveProgress,
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
  const copyTable = async (sheet?: string) => {
    if (!plan || copyingSheet !== null) return;
    setCopyingSheet(sheet ?? "");
    setError("");
    try {
      const payload = await bridge.call("engine/copyTable", {
        planId: plan.planId,
        sheet,
      });
      setError(s.t("rfi.editor.copied", { count: payload.count }));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setCopyingSheet(null);
    }
  };
  const prepareRfiReply = async () => {
    if (!plan || !result?.journalId) return;
    setError("");
    try {
      onPrepareRfiReply(
        await bridge.call("engine/rfiReplyPreview", {
          planId: plan.planId,
          journalId: result.journalId,
        }),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const undo = async () => {
    if (!result?.journalId) return;
    try {
      await bridge.call("engine/undo", {
        journalId: result.journalId,
      });
      onBack();
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        if (stage === "form") {
          event.preventDefault();
          void preview();
        } else if (stage === "preview" && !cancelling) {
          event.preventDefault();
          void apply();
        }
      } else if (event.key === "Escape") {
        if (stage === "previewing") {
          event.preventDefault();
          void cancelPreview();
        } else if (stage === "preview" || stage === "result") {
          event.preventDefault();
          setStage("form");
        } else if (stage === "form") {
          event.preventDefault();
          onBack();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [stage, cancelling, preview, apply, cancelPreview, onBack]);

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
                        value={values[param.name]}
                        values={values}
                        s={s}
                        onChange={(value) => {
                          valuesRevision.current += 1;
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
                  <section className="rfi-print-finder" aria-labelledby="rfi-print-title">
                    <h3 id="rfi-print-title">{s.t("print.rfi.title")}</h3>
                    {rfiInputs.length > 0 && (
                      <fieldset className="rfi-print-inputs">
                        <legend>{s.t("print.rfi.inputs")}</legend>
                        <div className="rfi-print-actions">
                          <Button
                            variant="ghost"
                            onClick={() => {
                              invalidateRfiFinder();
                              setExcludedRfiInputs([]);
                            }}
                          >
                            {s.t("action.selectAll")}
                          </Button>
                          <Button
                            variant="ghost"
                            onClick={() => {
                              invalidateRfiFinder();
                              setExcludedRfiInputs(rfiInputs);
                            }}
                          >
                            {s.t("action.selectNone")}
                          </Button>
                        </div>
                        {rfiInputs.map((input) => (
                          <label className="rfi-print-row" key={input}>
                            <input
                              type="checkbox"
                              checked={!excludedRfiInputs.includes(input)}
                              onChange={(event) => {
                                invalidateRfiFinder();
                                setExcludedRfiInputs((current) =>
                                  event.target.checked
                                    ? current.filter((value) => value !== input)
                                    : [...current, input],
                                );
                              }}
                            />
                            <span>{input}</span>
                          </label>
                        ))}
                      </fieldset>
                    )}
                    <Button
                      variant="secondary"
                      disabled={rfiFinderBusy || !rfiInputs.length}
                      onClick={() => void searchRfiDocuments()}
                    >
                      {s.t(rfiFinderBusy ? "print.rfi.searching" : "print.rfi.search")}
                    </Button>
                    {rfiFinderError && <Notice kind="error">{rfiFinderError}</Notice>}
                    {rfiPrintResolutions.length > 0 && (
                      <div className="rfi-print-results">
                        <p>{s.t("print.rfi.found", { count: rfiPrintCandidates.length })}</p>
                        {rfiSearchTruncated && <Notice kind="warning">{s.t("print.rfi.truncated")}</Notice>}
                        {rfiPrintResolutions.map((resolution, index) => (
                          <div className="rfi-print-resolution" key={`${resolution.requested}-${index}`}>
                            <strong>{resolution.requested}</strong>
                            {resolution.ambiguous && (
                              <p role="status">
                                {s.t("print.rfi.ambiguous")}
                                {resolution.rfiCodes.length > 0 && ` — ${resolution.rfiCodes.join(", ")}`}
                              </p>
                            )}
                            {resolution.missingFirstPage && <p>{s.t("print.rfi.missingFirstPage")}</p>}
                            {resolution.missingSketch && <p>{s.t("print.rfi.missingSketch")}</p>}
                            <Button
                              variant="ghost"
                              onClick={() =>
                                setRfiCandidateGroup(
                                  (candidate) => candidate.requested === resolution.requested,
                                  !rfiPrintCandidates
                                    .filter((candidate) => candidate.requested === resolution.requested)
                                    .every((candidate) => candidate.selected),
                                )
                              }
                            >
                              {s.t("print.rfi.toggleRequest")}
                            </Button>
                          </div>
                        ))}
                        {rfiFinderWasRun && (
                          <>
                            <div className="rfi-print-actions">
                              <Button variant="ghost" onClick={() => setRfiCandidateGroup(() => true, true)}>
                                {s.t("action.selectAll")}
                              </Button>
                              <Button variant="ghost" onClick={() => setRfiCandidateGroup(() => true, false)}>
                                {s.t("action.selectNone")}
                              </Button>
                              <Button
                                variant="ghost"
                                onClick={() => setRfiCandidateGroup((candidate) => candidate.documentType === "firstPage", true)}
                              >
                                {s.t("print.rfi.selectTypeFirstPage")}
                              </Button>
                              <Button
                                variant="ghost"
                                onClick={() => setRfiCandidateGroup((candidate) => candidate.documentType === "sketch", true)}
                              >
                                {s.t("print.rfi.selectTypeSketch")}
                              </Button>
                              <Button
                                variant="ghost"
                                onClick={() => setRfiCandidateGroup((candidate) => candidate.documentType === "firstPage", false)}
                              >
                                {s.t("print.rfi.clearTypeFirstPage")}
                              </Button>
                              <Button
                                variant="ghost"
                                onClick={() => setRfiCandidateGroup((candidate) => candidate.documentType === "sketch", false)}
                              >
                                {s.t("print.rfi.clearTypeSketch")}
                              </Button>
                            </div>
                            {[...new Set(rfiPrintCandidates.map((candidate) => candidate.rfiCode.match(/-RFI-([A-Z0-9]+)-/)?.[1]).filter((discipline): discipline is string => Boolean(discipline)))].map((discipline) => (
                              <Button
                                key={discipline}
                                variant="ghost"
                                onClick={() => setRfiCandidateGroup((candidate) => candidate.rfiCode.includes(`-RFI-${discipline}-`), true)}
                              >
                                {s.t("print.rfi.selectDiscipline", { discipline })}
                              </Button>
                            ))}
                          </>
                        )}
                        {rfiPrintCandidates.map((candidate) => (
                          <label
                            className="rfi-print-row"
                            key={`${candidate.requested}|${candidate.rfiCode}|${candidate.documentType}|${candidate.displayPath}`}
                          >
                            <input
                              type="checkbox"
                              checked={candidate.selected}
                              aria-label={`${candidate.rfiCode} ${s.t(`print.rfi.type.${candidate.documentType}`)} ${candidate.displayPath}`}
                              onChange={(event) =>
                                setRfiCandidateGroup(
                                  (row) =>
                                    row.requested === candidate.requested &&
                                    row.rfiCode === candidate.rfiCode &&
                                    row.documentType === candidate.documentType &&
                                    row.displayPath === candidate.displayPath,
                                  event.target.checked,
                                )
                              }
                            />
                            <span>
                              <strong>{candidate.rfiCode}</strong> — {s.t(`print.rfi.type.${candidate.documentType}`)}
                              <small>{candidate.displayPath}</small>
                            </span>
                          </label>
                        ))}
                        {rfiPrintCandidates.length === 0 && <p>{s.t("print.rfi.noMatches")}</p>}
                      </div>
                    )}
                  </section>
                )}
                {latestPrintQueue && (
                  <div className="resume-queue-card">
                    <div>
                      <strong>{s.t("print.resume.title")}</strong>
                      <small>
                        {s.t("print.resume.summary", {
                          count: latestPrintQueue.resumable,
                        })}
                      </small>
                    </div>
                    <span className="resume-queue-actions">
                      <Button
                        variant="ghost"
                        onClick={() =>
                          void discardPrintQueue(latestPrintQueue.queueId)
                        }
                      >
                        {s.t("print.resume.discard")}
                      </Button>
                      <Button
                        variant="secondary"
                        icon={<RotateCcw />}
                        onClick={() =>
                          void resumePrintQueue(latestPrintQueue.queueId)
                        }
                      >
                        {s.t("print.resume.review")}
                      </Button>
                    </span>
                  </div>
                )}
              </section>
              {error && <Notice kind="error">{error}</Notice>}
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
            onOpenDestination={
              (() => {
                const dest =
                  (isPathSelection(values.destination) && values.destination) ||
                  (isPathSelection(values.dest) && values.dest) ||
                  (isPathSelection(values.target) && values.target) ||
                  (isPathSelection(values.output_directory) &&
                    values.output_directory) ||
                  (isPathSelection(values.rfi_system) && values.rfi_system);
                return dest
                  ? () => void revealPath(bridge, dest.selectionId)
                  : undefined;
              })()
            }
          />
        )}
      </main>
    </div>
  );
}
