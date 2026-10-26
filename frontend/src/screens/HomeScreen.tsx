import {
  BadgeCheck,
  CheckCircle2,
  ChevronRight,
  CircleAlert,
  FileCheck2,
  FileSearch,
  FolderOpen,
  LogOut,
  Monitor,
  Moon,
  Printer,
  RefreshCw,
  RotateCcw,
  Send,
  Shield,
  Sun,
  TableProperties,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Button,
  Header,
  IconButton,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  type AppPrefs,
  type AuthState,
  type Bootstrap,
  type ThemePreference,
} from "../types";
import {
  Route,
} from "../helpers";

export type OperationCategory = "all" | "scans" | "excel" | "mail" | "system";

export function getOperationCategory(id: string): OperationCategory {
  if (
    id === "rfi.process_scans_fast_covers" ||
    id === "rfi.normalize_pdf_orientation" ||
    id === "rfi.stage_signed_first_pages"
  ) {
    return "scans";
  }
  if (
    id === "rfi.log_add" ||
    id === "rfi.follow_up_sync" ||
    id === "rfi.complete_status_sync" ||
    id === "rfi.publish_status" ||
    id === "rfi.system_sync"
  ) {
    return "excel";
  }
  if (id === "rfi.group_files") {
    return "mail";
  }
  return "system";
}

export function getOperationIcon(id: string) {
  switch (id) {
    case "files.batch_print":
      return <Printer />;
    case "rfi.group_files":
      return <FolderOpen />;
    case "rfi.log_add":
      return <TableProperties />;
    case "rfi.follow_up_sync":
      return <RefreshCw />;
    case "rfi.complete_status_sync":
      return <CheckCircle2 />;
    case "rfi.publish_status":
      return <Send />;
    case "rfi.stage_signed_first_pages":
      return <BadgeCheck />;
    case "rfi.system_sync":
      return <Shield />;
    case "rfi.normalize_pdf_orientation":
      return <RotateCcw />;
    case "rfi.process_scans_fast_covers":
      return <FileCheck2 />;
    default:
      return <FileSearch />;
  }
}


export function HomeScreen({
  bridge,
  bootstrap,
  s,
  onAuth,
  onPrefs,
  navigate,
}: {
  bridge: Bridge;
  bootstrap: Bootstrap;
  s: Strings;
  onAuth: (auth: AuthState) => void;
  onPrefs?: (prefs: AppPrefs) => void;
  navigate: (route: Route) => void;
}) {
  const signOut = async () => onAuth(await bridge.call("auth/signOut"));
  const [searchQuery, setSearchQuery] = useState("");
  const [activeCategory, setActiveCategory] =
    useState<OperationCategory>("all");

  const filteredOperations = useMemo(() => {
    return bootstrap.operations.filter((op) => {
      if (
        activeCategory !== "all" &&
        getOperationCategory(op.id) !== activeCategory
      ) {
        return false;
      }
      if (!searchQuery.trim()) return true;
      const q = searchQuery.trim().toLowerCase();
      const title = s.t(op.titleKey).toLowerCase();
      const desc = s.t(op.descriptionKey).toLowerCase();
      return (
        title.includes(q) || desc.includes(q) || op.id.toLowerCase().includes(q)
      );
    });
  }, [bootstrap.operations, activeCategory, searchQuery, s]);
  const primaryScan = bootstrap.operations.find(
    (operation) => operation.id === "rfi.process_scans_fast_covers",
  );
  const promotePrimaryScan =
    activeCategory === "all" && !searchQuery.trim();
  const listedOperations =
    promotePrimaryScan && primaryScan
      ? filteredOperations.filter((operation) => operation.id !== primaryScan.id)
      : filteredOperations;


  const latestTheme = useRef(bootstrap.prefs.theme);
  latestTheme.current = bootstrap.prefs.theme;

  const toggleTheme = useCallback(async () => {
    if (!onPrefs) return;
    const current = latestTheme.current;
    const nextTheme: ThemePreference =
      current === "system" ? "light" : current === "light" ? "dark" : "system";
    try {
      const updated = await bridge.call("settings/update", {
        theme: nextTheme,
      });
      onPrefs(updated);
    } catch {
      // Ignore background failure
    }
  }, [bridge, onPrefs]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.altKey && (event.key === "t" || event.key === "T")) {
        event.preventDefault();
        void toggleTheme();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleTheme]);

  return (
    <div className="screen">
      <Header
        trailing={
          <>
            <IconButton
              label={s.t("action.toggleTheme")}
              onClick={() => void toggleTheme()}
            >
              {bootstrap.prefs.theme === "dark" ? (
                <Moon />
              ) : bootstrap.prefs.theme === "light" ? (
                <Sun />
              ) : (
                <Monitor />
              )}
            </IconButton>
            <IconButton
              label={s.t("auth.signOut")}
              onClick={() => void signOut()}
            >
              <LogOut />
            </IconButton>
          </>
        }
      />
      <main className="dashboard-pane">
        <section className="page-heading">
          <div>
            <p className="eyebrow">DOCFLOW</p>
            <h1>{s.t("home.title")}</h1>
            <p>{s.t("home.subtitle")}</p>
          </div>
          <div className="safe-badge">
            <Shield /> {s.t("home.previewFirst")}{" "}
            <span aria-hidden="true">·</span>
            <span>v{bootstrap.version}</span>
          </div>
        </section>
        {primaryScan && promotePrimaryScan && (
          <button
            type="button"
            className="primary-scan-card"
            onClick={() =>
              navigate({ name: "operation", operation: primaryScan })
            }
          >
            <span className="primary-scan-icon">
              {getOperationIcon(primaryScan.id)}
            </span>
            <span className="primary-scan-copy">
              <strong>{s.t(primaryScan.titleKey)}</strong>
              <small>{s.t(primaryScan.descriptionKey)}</small>
            </span>
            <ChevronRight aria-hidden="true" />
          </button>
        )}
        <div className="dashboard-grid workspace-dashboard-grid">
          <section className="dashboard-section">
            <div className="section-heading">
              <div>
                <h2>{s.t("ops.heading")}</h2>
                <p>{s.t("ops.subheading")}</p>
              </div>
              <span className="section-count">
                {filteredOperations.length} / {bootstrap.operations.length}
              </span>
            </div>
            <div className="operation-search-bar">
              <FileSearch className="search-icon" />
              <input
                type="text"
                className="operation-search-input"
                placeholder={s.t("ops.search.placeholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              {searchQuery && (
                <button
                  type="button"
                  className="search-clear-btn"
                  onClick={() => setSearchQuery("")}
                  aria-label={s.t("action.cancel")}
                >
                  <X />
                </button>
              )}
            </div>
            <div className="category-filter-chips">
              {(["all", "scans", "excel", "mail", "system"] as const).map(
                (cat) => (
                  <button
                    key={cat}
                    type="button"
                    className={`category-chip ${activeCategory === cat ? "active" : ""}`}
                    onClick={() => setActiveCategory(cat)}
                  >
                    {s.t(`ops.category.${cat}`)}
                    <span className="chip-count">
                      {cat === "all"
                        ? bootstrap.operations.length
                        : bootstrap.operations.filter(
                            (o) => getOperationCategory(o.id) === cat,
                          ).length}
                    </span>
                  </button>
                ),
              )}
            </div>
            {filteredOperations.length === 0 ? (
              <div className="empty-search-state">
                <CircleAlert />
                <p>
                  {s.t("ops.emptySearch", {
                    query: searchQuery,
                  })}
                </p>
                <Button
                  variant="secondary"
                  onClick={() => {
                    setSearchQuery("");
                    setActiveCategory("all");
                  }}
                >
                  {s.t("action.cancel")}
                </Button>
              </div>
            ) : (
              <div className="operation-list">
                {listedOperations.map((operation) => (
                  <button
                    className="operation-card"
                    key={operation.id}
                    onClick={() => navigate({ name: "operation", operation })}
                  >
                    <span className="operation-icon">
                      {getOperationIcon(operation.id)}
                    </span>
                    <span className="operation-copy">
                      <div className="operation-header-row">
                        <strong>{s.t(operation.titleKey)}</strong>
                        <span className="category-tag">
                          {s.t(
                            `ops.category.${getOperationCategory(operation.id)}`,
                          )}
                        </span>
                      </div>
                      <small>{s.t(operation.descriptionKey)}</small>
                    </span>
                    <ChevronRight />
                  </button>
                ))}
              </div>
            )}
          </section>
        </div>
      </main>
    </div>
  );
}
