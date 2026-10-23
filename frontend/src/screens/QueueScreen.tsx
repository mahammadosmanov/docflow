import {
  CheckCircle2,
  CircleAlert,
  Eye,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  BusyState,
  Button,
  EmptyState,
  Header,
  Notice,
} from "../components/ui";
import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import {
  type OperationDef,
  type Plan,
  type WatchItem,
  type WatchStatus,
} from "../types";
import { humanError } from "../helpers";

const PREPARING_REFRESH_MS = 5000;

function failureMessage(item: WatchItem, s: Strings): string {
  const code = item.errorCode ?? "";
  if (code.startsWith("watch.")) return s.t(`error.${code}`);
  return s.t("queue.failed.generic", { code });
}

export function QueueScreen({
  bridge,
  operations,
  s,
  onBack,
  onOpenPlan,
}: {
  bridge: Bridge;
  operations: OperationDef[];
  s: Strings;
  onBack: () => void;
  onOpenPlan: (
    operation: OperationDef,
    plan: Plan,
    values: Record<string, unknown>,
  ) => void;
}) {
  const [status, setStatus] = useState<WatchStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const next = await bridge.call("watch/status");
      if (mounted.current) setStatus(next);
    } catch (reason) {
      if (mounted.current) setError(humanError(reason, s));
    }
  }, [bridge, s]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const [burst, setBurst] = useState(0);
  const preparing = status?.items.some((item) => item.status === "preparing");
  const refreshing = Boolean(preparing) || burst > 0;
  useEffect(() => {
    if (!refreshing) return undefined;
    const timer = window.setInterval(() => {
      setBurst((current) => Math.max(0, current - 1));
      void refresh();
    }, PREPARING_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [refreshing, refresh]);

  const update = async (
    opId: string,
    change: {
      enabled?: boolean;
      action?: "checkNow" | "dismiss" | "reprepare";
    },
  ) => {
    setBusy(true);
    setError("");
    try {
      const next = await bridge.call("watch/update", { opId, ...change });
      if (mounted.current) {
        setStatus(next);
        if (change.action !== "dismiss" && change.enabled !== false) {
          setBurst(6);
        }
      }
    } catch (reason) {
      if (mounted.current) setError(humanError(reason, s));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const checkNow = async () => {
    const first = status?.enabled[0];
    if (!first) return;
    await update(first, { action: "checkNow" });
  };

  const open = async (operation: OperationDef) => {
    setBusy(true);
    setError("");
    try {
      const plan = await bridge.call("watch/open", { opId: operation.id });
      const values = await bridge.call("settings/params", {
        opId: operation.id,
      });
      onOpenPlan(operation, plan, values);
    } catch (reason) {
      setError(humanError(reason, s));
      await refresh();
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const titleOf = (opId: string) => {
    const operation = operations.find((candidate) => candidate.id === opId);
    return operation ? s.t(operation.titleKey) : opId;
  };
  const items = status?.items ?? [];

  return (
    <div className="screen">
      <Header
        title={s.t("queue.title")}
        onBack={onBack}
        backLabel={s.t("action.back")}
      />
      <main className="pane">
        <div className="page-heading compact-heading">
          <div>
            <h1>{s.t("queue.title")}</h1>
            <p>{s.t("queue.help")}</p>
          </div>
          <Button
            variant="secondary"
            icon={<RefreshCw />}
            disabled={busy || !status || status.enabled.length === 0}
            onClick={() => void checkNow()}
          >
            {s.t("queue.checkNow")}
          </Button>
        </div>
        {!status ? (
          <BusyState label={s.t("queue.title")} s={s} />
        ) : (
          <div className="flow-layout">
            {status.paused && (
              <Notice kind="info">{s.t("queue.paused")}</Notice>
            )}
            {items.length === 0 && (
              <EmptyState
                icon={<CheckCircle2 />}
                title={s.t("queue.empty")}
                message={s.t("queue.empty.help")}
              />
            )}
            {items.map((item) => {
              const operation = operations.find(
                (candidate) => candidate.id === item.opId,
              );
              return (
                <div className="history-row" key={item.opId}>
                  <span className="history-icon">
                    {item.status === "preparing" ? (
                      <LoaderCircle className="spin" />
                    ) : item.status === "ready" ? (
                      <Eye />
                    ) : (
                      <CircleAlert />
                    )}
                  </span>
                  <div className="history-copy">
                    <strong>{titleOf(item.opId)}</strong>
                    <small>
                      {item.status === "ready"
                        ? s.t("queue.summary", {
                            steps: item.steps,
                            warnings: item.warnings,
                          })
                        : item.status === "failed"
                          ? failureMessage(item, s)
                          : s.t(`queue.status.${item.status}`)}
                    </small>
                  </div>
                  <span className="resume-queue-actions">
                    <span className="status-label">
                      {s.t(`queue.status.${item.status}`)}
                    </span>
                    {item.status === "ready" && operation && (
                      <Button
                        variant="secondary"
                        icon={<Eye />}
                        disabled={busy}
                        onClick={() => void open(operation)}
                      >
                        {s.t("queue.open")}
                      </Button>
                    )}
                    {(item.status === "stale" || item.status === "failed") && (
                      <Button
                        variant="secondary"
                        icon={<RotateCcw />}
                        disabled={busy}
                        onClick={() =>
                          void update(item.opId, { action: "reprepare" })
                        }
                      >
                        {s.t("queue.reprepare")}
                      </Button>
                    )}
                    {item.status !== "preparing" && (
                      <Button
                        variant="ghost"
                        icon={<X />}
                        disabled={busy}
                        onClick={() =>
                          void update(item.opId, { action: "dismiss" })
                        }
                      >
                        {s.t("queue.dismiss")}
                      </Button>
                    )}
                  </span>
                </div>
              );
            })}
            <fieldset className="queue-watch-list">
              {status.operations.map((opId) => {
                const checkboxId = `queue-watch-${opId}`;
                return (
                  <label className="queue-watch-row" htmlFor={checkboxId} key={opId}>
                    <input
                      id={checkboxId}
                      type="checkbox"
                      checked={status.enabled.includes(opId)}
                      disabled={busy}
                      onChange={(event) =>
                        void update(opId, { enabled: event.target.checked })
                      }
                    />
                    <span>
                      <strong>{titleOf(opId)}</strong>
                      <small>{s.t("queue.watch")}</small>
                    </span>
                  </label>
                );
              })}
            </fieldset>
          </div>
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </main>
    </div>
  );
}
