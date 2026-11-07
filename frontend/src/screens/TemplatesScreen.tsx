import {
  CloudOff,
  FileSearch,
  FileSpreadsheet,
  FileText,
  LibraryBig,
  RefreshCw,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  Header,
  IconButton,
} from "../components/ui";
import {
  EmptyState,
  Notice,
} from "../components/feedback";
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
  type TemplateStatus,
} from "../types";
import {
  formatDate,
  humanError,
} from "../helpers";

/** Shared document templates that Rust syncs and caches for offline use (ADR 0055). */
export function TemplatesScreen({
  bridge,
  s,
  onBack,
}: {
  bridge: Bridge;
  s: Strings;
  onBack: () => void;
}) {
  const [status, setStatus] = useState<TemplateStatus | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [discipline, setDiscipline] = useState("all");

  const run = useCallback(
    async (sync: boolean) => {
      setSyncing(sync);
      setError("");
      try {
        setStatus(
          sync
            ? await bridge.call("templates/sync")
            : await bridge.call("templates/status"),
        );
      } catch (reason) {
        setError(humanError(reason, s));
      } finally {
        setSyncing(false);
      }
    },
    [bridge, s],
  );

  useEffect(() => {
    void run(false).then(() => run(true));
  }, [run]);

  const disciplines = useMemo(
    () => [...new Set(status?.templates.map((item) => item.discipline))].sort(),
    [status],
  );
  const needle = query.trim().toLowerCase();
  const visible = (status?.templates ?? []).filter(
    (item) =>
      (discipline === "all" || item.discipline === discipline) &&
      (!needle ||
        item.code.toLowerCase().includes(needle) ||
        item.title.toLowerCase().includes(needle)),
  );
  const lastSynced =
    status?.lastSyncedAt != null
      ? formatDate(new Date(status.lastSyncedAt * 1000).toISOString(), s.locale)
      : null;

  return (
    <div className="screen">
      <Header
        title={s.t("templates.title")}
        onBack={onBack}
        backLabel={s.t("action.back")}
        trailing={
          <IconButton
            label={s.t("templates.sync")}
            disabled={syncing}
            onClick={() => void run(true)}
          >
            <RefreshCw />
          </IconButton>
        }
      />
      <main className="pane">
        <div className="page-heading compact-heading">
          <div>
            <h1>{s.t("templates.title")}</h1>
            <p>{s.t("templates.help")}</p>
          </div>
        </div>
        {status && (
          <p className="templates-sync-line" role="status">
            {status.online ? (
              <RefreshCw aria-hidden="true" />
            ) : (
              <CloudOff aria-hidden="true" />
            )}
            <span>
              {syncing
                ? s.t("templates.syncing")
                : status.online
                  ? s.t("templates.synced", { when: lastSynced ?? "" })
                  : lastSynced
                    ? s.t("templates.offlineSince", { when: lastSynced })
                    : s.t("templates.neverSynced")}
            </span>
          </p>
        )}
        {status?.lastError && status.lastError !== "offline" && (
          <Notice kind="warning">
            {s.t(`templates.error.${status.lastError}`)}
          </Notice>
        )}
        {status && status.failedCodes.length > 0 && (
          <Notice kind="warning">
            {s.t("templates.failed", { codes: status.failedCodes.join(", ") })}
          </Notice>
        )}
        {!status ? (
          <BusyState label={s.t("templates.title")} s={s} />
        ) : status.templates.length === 0 ? (
          <EmptyState
            icon={<LibraryBig />}
            title={s.t("templates.empty")}
            message={s.t("templates.empty.help")}
          />
        ) : (
          <div className="flow-layout">
            <div className="operation-search-bar">
              <FileSearch className="search-icon" />
              <input
                type="text"
                className="operation-search-input"
                aria-label={s.t("templates.search")}
                placeholder={s.t("templates.search")}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              {query && (
                <button
                  type="button"
                  className="search-clear-btn"
                  onClick={() => setQuery("")}
                  aria-label={s.t("action.cancel")}
                >
                  <X />
                </button>
              )}
            </div>
            <div className="category-filter-chips">
              {["all", ...disciplines].map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`category-chip ${discipline === value ? "active" : ""}`}
                  aria-pressed={discipline === value}
                  onClick={() => setDiscipline(value)}
                >
                  {value === "all" ? s.t("ops.category.all") : value}
                  <span className="chip-count">
                    {value === "all"
                      ? status.templates.length
                      : status.templates.filter(
                          (item) => item.discipline === value,
                        ).length}
                  </span>
                </button>
              ))}
            </div>
            {visible.map((item) => (
              <div className="history-row" key={item.code}>
                <span className="history-icon">
                  {item.fileKind === "xlsx" ? <FileSpreadsheet /> : <FileText />}
                </span>
                <div className="history-copy">
                  <strong>{item.code}</strong>
                  <small>{item.title}</small>
                </div>
                <span className="template-meta">
                  <span className="category-tag">{item.discipline}</span>
                  <span className="status-label">
                    {s.t("templates.version", { version: item.version })}
                  </span>
                  <span className="status-label">
                    {s.t("templates.fields", { count: item.fieldCount })}
                  </span>
                  <span
                    className={`status-label ${item.cached ? "" : "warning"}`}
                  >
                    {s.t(item.cached ? "templates.cached" : "templates.notCached")}
                  </span>
                </span>
              </div>
            ))}
            {visible.length === 0 && (
              <p className="muted">{s.t("templates.noMatch")}</p>
            )}
          </div>
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </main>
    </div>
  );
}
