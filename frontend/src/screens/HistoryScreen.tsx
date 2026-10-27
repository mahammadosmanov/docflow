import {
  Check,
  FolderOpen,
  History,
  RefreshCw,
  Undo2,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useState,
} from "react";
import {
  BusyState,
  Button,
  EmptyState,
  Header,
  IconButton,
  Modal,
  Notice,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  chooseFolder,
} from "../lib/platform";
import {
  type HistoryPayload,
  type HistoryRow,
  type OperationDef,
  type PathSelection,
} from "../types";
import {
  formatDate,
  humanError,
} from "../helpers";

export function HistoryScreen({
  bridge,
  s,
  operations,
  onBack,
}: {
  bridge: Bridge;
  s: Strings;
  operations: OperationDef[];
  onBack: () => void;
}) {
  const [payload, setPayload] = useState<HistoryPayload | null>(null);
  const [undoRow, setUndoRow] = useState<HistoryRow | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const load = useCallback(async () => {
    setBusy(true);
    try {
      setPayload(await bridge.call("history/list", { limit: 50 }));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  }, [bridge, s]);
  useEffect(() => {
    void load();
  }, [load]);

  const undo = async (row: HistoryRow, destination?: PathSelection) => {
    setUndoRow(null);
    setBusy(true);
    try {
      await bridge.call("engine/undo", {
        journalId: row.journalId,
        cloudId: row.cloudId,
        ...(destination ? { destination } : {}),
      });
      await load();
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="screen">
      <Header
        title={s.t("history.title")}
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
            <h1>{s.t("history.title")}</h1>
            <p>
              {payload?.online
                ? s.t("history.sourceCloud")
                : s.t("history.sourceLocal")}
            </p>
          </div>
          <span className="safe-badge">
            {payload?.online ? <Check /> : <History />}
            {s.t(payload?.online ? "history.cloud" : "history.local")}
          </span>
        </div>
        {busy && !payload ? (
          <BusyState label={s.t("history.title")} s={s} />
        ) : payload?.rows.length ? (
          <div className="history-list">
            {payload.rows.map((row) => (
              <div
                className="history-row"
                key={`${row.cloudId}-${row.journalId}-${row.when}`}
              >
                <span className="history-icon">
                  <History />
                </span>
                <div className="history-copy">
                  <strong>
                    {s.t(
                      row.opId === "rfi.review"
                        ? "review.title"
                        : row.opId === "rfi.process_scans"
                          ? "op.rfi.process_scans.title"
                          : (operations.find(
                                (operation) => operation.id === row.opId,
                              )?.titleKey ?? "history.unknownOperation"),
                    )}
                  </strong>
                  <small>
                    {formatDate(row.when, s.locale)} ·{" "}
                    {s.t("history.moves", { count: row.moves })}
                    {row.userEmail ? ` · ${row.userEmail}` : ""}
                  </small>
                </div>
                {row.undone ? (
                  <span className="status-label">
                    <Check /> {s.t("history.undone")}
                  </span>
                ) : !row.undoAvailable ? (
                  <span className="status-label">
                    {s.t("history.localJournalRequired")}
                  </span>
                ) : (
                  <Button
                    variant="secondary"
                    icon={<Undo2 />}
                    disabled={busy}
                    onClick={() => setUndoRow(row)}
                  >
                    {s.t("action.undo")}
                  </Button>
                )}
              </div>
            ))}
          </div>
        ) : (
          <EmptyState
            icon={<History />}
            title={s.t("history.empty")}
            message={s.t("history.empty.help")}
          />
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </main>
      {undoRow && (
        <Modal
          title={s.t("action.undo")}
          closeLabel={s.t("action.close")}
          onClose={() => setUndoRow(null)}
        >
          <p className="muted">
            {s.t("history.moves", { count: undoRow.moves })}
          </p>
          <div className="modal-actions vertical">
            <Button icon={<Undo2 />} onClick={() => void undo(undoRow)}>
              {s.t("history.undoOriginal")}
            </Button>
            <Button
              variant="secondary"
              icon={<FolderOpen />}
              onClick={() =>
                void chooseFolder(bridge, "history.undo", "destination").then(
                  (folder) => {
                    if (folder) return undo(undoRow, folder);
                  },
                )
              }
            >
              {s.t("history.undoToFolder")}
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
