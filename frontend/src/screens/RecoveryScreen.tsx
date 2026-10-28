import {
  CheckCircle2,
  CircleAlert,
  RefreshCw,
  RotateCcw,
  Undo2,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useState,
} from "react";
import {
  Button,
  EmptyState,
  Header,
  IconButton,
  Notice,
} from "../components/ui";
import {
  BusyState,
} from "../components/BusyState";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  type OperationDef,
  type RecoveryPayload,
} from "../types";
import {
  humanError,
} from "../helpers";

export function RecoveryScreen({
  bridge,
  operations,
  s,
  onBack,
  onOpenOperation,
}: {
  bridge: Bridge;
  operations: OperationDef[];
  s: Strings;
  onBack: () => void;
  onOpenOperation: (operation: OperationDef) => void;
}) {
  const [payload, setPayload] = useState<RecoveryPayload | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      setPayload(await bridge.call("recovery/list"));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  }, [bridge, s]);

  useEffect(() => {
    void load();
  }, [load]);

  const discardQueue = async () => {
    if (!payload?.printQueue) return;
    setBusy(true);
    setError("");
    try {
      await bridge.call("engine/printQueueDiscard", {
        queueId: payload.printQueue.queueId,
      });
      await load();
    } catch (reason) {
      setError(humanError(reason, s));
      setBusy(false);
    }
  };

  const restoreOriginal = async (journalId: string) => {
    setBusy(true);
    setError("");
    try {
      const result = await bridge.call("engine/undo", { journalId });
      setPayload(await bridge.call("recovery/list"));
      if (result.failed > 0) {
        setError(s.t("error.rfi.followUp.recoveryRequired"));
      }
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  const openPrintQueue = () => {
    const operation = operations.find(
      (candidate) => candidate.id === "files.batch_print",
    );
    if (operation) onOpenOperation(operation);
  };

  return (
    <div className="screen">
      <Header
        title={s.t("recovery.title")}
        onBack={onBack}
        backLabel={s.t("action.back")}
        trailing={
          <IconButton label={s.t("action.retry")} onClick={() => void load()}>
            <RefreshCw />
          </IconButton>
        }
      />
      <main className="pane">
        <div className="page-heading compact-heading">
          <div>
            <h1>{s.t("recovery.title")}</h1>
            <p>{s.t("recovery.help")}</p>
          </div>
        </div>
        {busy && !payload ? (
          <BusyState label={s.t("recovery.title")} s={s} />
        ) : (
          <div className="flow-layout">
            {payload?.journals.map((entry) => (
              <div className="history-row" key={entry.journalId}>
                <span className="history-icon">
                  <CircleAlert />
                </span>
                <div className="history-copy">
                  <strong>
                    {s.t(
                      entry.disposition === "no_effect"
                        ? "recovery.noEffect"
                        : "recovery.outcomeUnknown",
                    )}
                  </strong>
                  <small>{entry.journalId}</small>
                </div>
                <span className="resume-queue-actions">
                  <span className="status-label">{s.t("recovery.manual")}</span>
                  {entry.restoreOriginalAvailable && (
                    <Button
                      variant="secondary"
                      icon={<Undo2 />}
                      disabled={busy}
                      onClick={() => void restoreOriginal(entry.journalId)}
                    >
                      {s.t("recovery.restoreOriginal")}
                    </Button>
                  )}
                </span>
              </div>
            ))}
            {payload?.printQueue && (
              <div className="resume-queue-card">
                <div>
                  <strong>{s.t("recovery.printQueue")}</strong>
                  <small>
                    {s.t("print.resume.summary", {
                      count: payload.printQueue.resumable,
                    })}
                  </small>
                </div>
                <span className="resume-queue-actions">
                  <Button
                    variant="ghost"
                    disabled={busy}
                    onClick={() => void discardQueue()}
                  >
                    {s.t("print.resume.discard")}
                  </Button>
                  <Button
                    variant="secondary"
                    icon={<RotateCcw />}
                    disabled={busy}
                    onClick={openPrintQueue}
                  >
                    {s.t("print.resume.review")}
                  </Button>
                </span>
              </div>
            )}
            {payload &&
              payload.journals.length === 0 &&
              !payload.printQueue && (
                <EmptyState
                  icon={<CheckCircle2 />}
                  title={s.t("recovery.empty")}
                  message={s.t("recovery.empty.help")}
                />
              )}
          </div>
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </main>
    </div>
  );
}
