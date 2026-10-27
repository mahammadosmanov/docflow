import {
  ArrowRight,
  CircleAlert,
  Eye,
  FileCheck2,
  FileSearch,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useState,
} from "react";
import {
  ActionBar,
  BusyState,
  Button,
  EmptyState,
  Header,
  Notice,
  Segmented,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  revealPath,
} from "../lib/platform";
import {
  type OperationResult,
  type PathSelection,
  type Plan,
  type ReviewAction,
  type ReviewDecision,
  type ReviewItem,
} from "../types";
import {
  humanError,
} from "../helpers";
import {
  formatConfidencePercent,
} from "../plan/EditablePlanTable";
import {
  PlanView,
} from "../plan/PlanView";
import {
  ResultView,
} from "./ResultView";

export function ReviewScreen({
  bridge,
  destination,
  s,
  onBack,
}: {
  bridge: Bridge;
  destination: PathSelection;
  s: Strings;
  onBack: () => void;
}) {
  const [items, setItems] = useState<ReviewItem[]>([]);
  const [actions, setActions] = useState<Record<string, ReviewAction>>({});
  const [codes, setCodes] = useState<Record<string, string>>({});
  const [plan, setPlan] = useState<Plan | null>(null);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");

  const scan = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const rows = await bridge.call("engine/reviewScan", {
        destination,
      });
      setItems(rows);
      setActions(
        Object.fromEntries(rows.map((item) => [item.selectionId, "keep"])),
      );
      setCodes(
        Object.fromEntries(rows.map((item) => [item.selectionId, item.code])),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  }, [bridge, destination, s]);

  useEffect(() => {
    void scan();
  }, [scan]);

  const decisions: ReviewDecision[] = items
    .filter((item) => actions[item.selectionId] !== "keep")
    .map((item) => ({
      itemId: item.selectionId,
      action: actions[item.selectionId] as ReviewDecision["action"],
      code: codes[item.selectionId] ?? "",
    }));

  const preview = async () => {
    setBusy(true);
    setError("");
    try {
      setPlan(
        await bridge.call("engine/reviewPreview", {
          destination,
          decisions,
        }),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  const apply = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      setResult(
        await bridge.call("engine/apply", {
          opId: "rfi.review",
          planId: plan.planId,
        }),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  const undo = async () => {
    if (!result?.journalId) return;
    await bridge.call("engine/undo", { journalId: result.journalId });
    setResult(null);
    setPlan(null);
    await scan();
  };

  return (
    <div className="screen">
      <Header
        title={s.t("review.title")}
        onBack={onBack}
        backLabel={s.t("action.back")}
      />
      <main className="work-pane review-pane">
        {result ? (
          <ResultView
            result={result}
            s={s}
            error={error}
            onDone={onBack}
            onUndo={() => void undo()}
            onOpenDestination={() =>
              void revealPath(bridge, destination.selectionId)
            }
          />
        ) : plan ? (
          <PlanView
            plan={plan}
            s={s}
            error={error}
            onCancel={() => setPlan(null)}
            onApply={() => void apply()}
          />
        ) : busy ? (
          <BusyState label={s.t("action.review")} s={s} />
        ) : (
          <div className="flow-layout">
            <div className="flow-scroll">
              <div className="page-heading compact-heading">
                <div>
                  <h1>{s.t("review.title")}</h1>
                  <p>{s.t("review.help")}</p>
                </div>
                <Button
                  variant="secondary"
                  icon={<RefreshCw />}
                  onClick={() => void scan()}
                >
                  {s.t("action.retry")}
                </Button>
              </div>
              {items.length ? (
                <>
                  <div className="review-batch-bar">
                    <span className="batch-label">{s.t("review.batch.label")}</span>
                    <div className="batch-actions">
                      <Button
                        variant="secondary"
                        icon={<RotateCcw />}
                        onClick={() => {
                          setActions(
                            Object.fromEntries(
                              items.map((it) => [it.selectionId, "keep"]),
                            ),
                          );
                        }}
                      >
                        {s.t("review.selectAllKeep")}
                      </Button>
                      <Button
                        variant="secondary"
                        icon={<ArrowRight />}
                        onClick={() => {
                          setActions(
                            Object.fromEntries(
                              items.map((it) => [
                                it.selectionId,
                                it.code.trim() ? "put" : "keep",
                              ]),
                            ),
                          );
                        }}
                      >
                        {s.t("review.selectAllPut")}
                      </Button>
                      <Button
                        variant="secondary"
                        icon={<Trash2 />}
                        onClick={() => {
                          setActions(
                            Object.fromEntries(
                              items.map((it) => [it.selectionId, "delete"]),
                            ),
                          );
                        }}
                      >
                        {s.t("review.selectAllTrash")}
                      </Button>
                    </div>
                  </div>
                  <div className="review-list">
                  {items.map((item) => (
                    <div className="review-row" key={item.selectionId}>
                      <button
                        type="button"
                        className="file-button"
                        title={item.path}
                        onClick={() =>
                          void revealPath(bridge, item.selectionId)
                        }
                      >
                        <FileSearch />
                        <span>
                          <strong>{item.name}</strong>
                          <small>{item.path}</small>
                          <small className="review-confidence">
                            {s.t("ocr.confidence")}:{" "}
                            {formatConfidencePercent(item.confidence) ?? "—"}
                          </small>
                        </span>
                      </button>
                      <input
                        aria-label={`${item.name} code`}
                        value={codes[item.selectionId] ?? ""}
                        placeholder={s.t("review.noCode")}
                        disabled={actions[item.selectionId] !== "put"}
                        onChange={(event) =>
                          setCodes((current) => ({
                            ...current,
                            [item.selectionId]: event.target.value,
                          }))
                        }
                      />
                      <Segmented
                        label={item.name}
                        value={actions[item.selectionId] ?? "keep"}
                        options={[
                          ["keep", s.t("review.keep")],
                          ["put", s.t("review.put")],
                          ["delete", s.t("review.delete")],
                        ]}
                        onChange={(action) =>
                          setActions((current) => ({
                            ...current,
                            [item.selectionId]: action as ReviewAction,
                          }))
                        }
                        small
                      />
                      {item.targetExists && actions[item.path] === "put" && (
                        <span className="row-warning">
                          <CircleAlert /> {s.t("review.willReplace")}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
                </>
              ) : (
                <EmptyState
                  icon={<FileCheck2 />}
                  title={s.t("review.empty")}
                  message={s.t("review.empty.help")}
                />
              )}
              {error && <Notice kind="error">{error}</Notice>}
            </div>
            <ActionBar
              left={
                <span className="muted">
                  {decisions.length
                    ? s.t("review.selected", {
                        count: decisions.length,
                      })
                    : s.t("review.noChanges")}
                </span>
              }
              right={
                <Button
                  icon={<Eye />}
                  disabled={!decisions.length || busy}
                  onClick={() => void preview()}
                >
                  {s.t("action.preview")}
                </Button>
              }
            />
          </div>
        )}
      </main>
    </div>
  );
}
