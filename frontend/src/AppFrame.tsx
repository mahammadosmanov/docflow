import {
  FileCheck2,
  Files,
  History,
  ListChecks,
  LoaderCircle,
  RotateCcw,
  Settings,
  UserRoundCog,
} from "lucide-react";
import {
  type ReactNode,
  useEffect,
  useState,
} from "react";
import {
  type Strings,
} from "./i18n";
import {
  type Bridge,
} from "./lib/bridge";
import {
  chooseFolder,
} from "./lib/platform";
import {
  type AppUser,
} from "./types";
import {
  Route,
  humanError,
} from "./helpers";
import {
} from "./screens/HomeScreen";
import {
} from "./screens/OperationScreen";
import {
} from "./screens/SettingsScreen";

export function AppFrame({
  bridge,
  routeName,
  isAdmin,
  user,
  s,
  onNavigate,
  children,
}: {
  bridge: Bridge;
  routeName: Route["name"];
  isAdmin: boolean;
  user: AppUser | null;
  s: Strings;
  onNavigate: (route: Route) => void;
  children: ReactNode;
}) {
  const [reviewError, setReviewError] = useState("");
  const [openingReview, setOpeningReview] = useState(false);
  const workspaceActive = routeName === "home" || routeName === "operation";
  const navigate = (next: Route) => {
    setReviewError("");
    onNavigate(next);
  };
  const openReview = async () => {
    setOpeningReview(true);
    setReviewError("");
    try {
      const destination = await chooseFolder(bridge, "rfi.review", "destination");
      if (destination) navigate({ name: "review", destination });
    } catch (error) {
      setReviewError(humanError(error, s));
    } finally {
      setOpeningReview(false);
    }
  };
  const [readyCount, setReadyCount] = useState(0);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const status = await bridge.call("watch/status");
        if (active) {
          setReadyCount(
            status.items.filter((item) => item.status === "ready").length,
          );
        }
      } catch {
        if (active) setReadyCount(0);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 60_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [bridge, routeName]);
  const profileName = user?.display || user?.email || "";

  return (
    <div className="app-frame">
      <aside className="app-rail">
        <div className="app-brand">
          <span className="app-brand-mark">
            <Files aria-hidden="true" />
          </span>
          <span>DocFlow</span>
        </div>
        <nav className="app-navigation" aria-label={s.t("app.title")}>
          <div className="app-nav-group">
            <button
              type="button"
              aria-label={s.t("home.title")}
              title={s.t("home.title")}
              className={`app-nav-item ${workspaceActive ? "active" : ""}`}
              aria-current={workspaceActive ? "page" : undefined}
              onClick={() => navigate({ name: "home" })}
            >
              <Files aria-hidden="true" />
              <span>{s.t("home.title")}</span>
            </button>
            <button
              type="button"
              aria-label={s.t("action.review")}
              title={s.t("action.review")}
              className={`app-nav-item ${routeName === "review" ? "active" : ""}`}
              aria-current={routeName === "review" ? "page" : undefined}
              aria-busy={openingReview}
              disabled={openingReview}
              onClick={() => void openReview()}
            >
              <FileCheck2 aria-hidden="true" />
              <span>{s.t("action.review")}</span>
            </button>
            <button
              type="button"
              className={`app-nav-item ${routeName === "queue" ? "active" : ""}`}
              aria-label={
                readyCount > 0
                  ? `${s.t("queue.title")}, ${s.t("queue.badge", { count: readyCount })}`
                  : s.t("queue.title")
              }
              title={s.t("queue.title")}
              aria-current={routeName === "queue" ? "page" : undefined}
              onClick={() => navigate({ name: "queue" })}
            >
              <ListChecks aria-hidden="true" />
              <span>{s.t("queue.title")}</span>
              {readyCount > 0 && (
                <span className="app-nav-badge" aria-hidden="true">
                  {readyCount}
                </span>
              )}
            </button>
            <button
              type="button"
              className={`app-nav-item ${routeName === "history" ? "active" : ""}`}
              aria-label={s.t("action.history")}
              title={s.t("action.history")}
              aria-current={routeName === "history" ? "page" : undefined}
              onClick={() => navigate({ name: "history" })}
            >
              <History aria-hidden="true" />
              <span>{s.t("action.history")}</span>
            </button>
            <button
              type="button"
              className={`app-nav-item ${routeName === "recovery" ? "active" : ""}`}
              aria-current={routeName === "recovery" ? "page" : undefined}
              aria-label={s.t("recovery.title")}
              title={s.t("recovery.title")}
              onClick={() => navigate({ name: "recovery" })}
            >
              <RotateCcw aria-hidden="true" />
              <span>{s.t("recovery.title")}</span>
            </button>
          </div>
          <div className="app-nav-divider" />
          <div className="app-nav-group">
            <button
              type="button"
              className={`app-nav-item ${routeName === "settings" ? "active" : ""}`}
              aria-current={routeName === "settings" ? "page" : undefined}
              aria-label={s.t("prefs.title")}
              title={s.t("prefs.title")}
              onClick={() => navigate({ name: "settings", back: "home" })}
            >
              <Settings aria-hidden="true" />
              <span>{s.t("prefs.title")}</span>
            </button>
            {isAdmin && (
              <button
                type="button"
                className={`app-nav-item ${routeName === "admin" ? "active" : ""}`}
                aria-current={routeName === "admin" ? "page" : undefined}
                onClick={() => navigate({ name: "admin" })}
                aria-label={s.t("action.admin")}
                title={s.t("action.admin")}
              >
                <UserRoundCog aria-hidden="true" />
                <span>{s.t("action.admin")}</span>
              </button>
            )}
          </div>
        </nav>
        {reviewError && (
          <div className="app-nav-error" role="alert">
            {reviewError}
          </div>
        )}
        <div className="app-profile">
          <span className="app-profile-avatar" aria-hidden="true">
            {profileName.slice(0, 1).toLocaleUpperCase()}
          </span>
          <span className="app-profile-copy">
            <strong>{profileName}</strong>
            <small>{user?.email}</small>
          </span>
        </div>
      </aside>
      <div className="app-frame-content">{children}</div>
    </div>
  );
}

export function Splash() {
  return (
    <div className="splash">
      <div className="brand-mark">
        <Files />
      </div>
      <strong>DocFlow</strong>
      <LoaderCircle className="spin muted" size={18} />
    </div>
  );
}
