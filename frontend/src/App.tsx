import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  BadgeCheck,
  CalendarDays,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleAlert,
  Copy,
  Database,
  Download,
  Eye,
  EyeOff,
  FileCheck2,
  FileSearch,
  Files,
  FolderOpen,
  History,
  LoaderCircle,
  LogOut,
  Mail,
  Monitor,
  Moon,
  Play,
  Plus,
  Printer,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  Shield,
  SlidersHorizontal,
  Sun,
  TableProperties,
  Trash2,
  Undo2,
  User,
  UserRoundCog,
  X,
  XCircle,
} from "lucide-react";
import {
  type FormEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  ActionBar,
  BusyState,
  Button,
  Centered,
  EmptyState,
  Field,
  Header,
  IconButton,
  Modal,
  Notice,
  Segmented,
  StepIndicator,
  Switch,
} from "./components/ui";
import { makeStrings, type Strings } from "./i18n";
import { getBridge, type Bridge } from "./lib/bridge";
import {
  chooseFile,
  chooseFiles,
  chooseFolder,
  dropPaths,
  revealPath,
} from "./lib/platform";
import type {
  AppPrefs,
  AppUser,
  AuthState,
  Bootstrap,
  EditorMetadata,
  HistoryPayload,
  HistoryRow,
  OperationDef,
  OperationPreset,
  OperationResult,
  ParamDef,
  PathSelection,
  Plan,
  PrintQueueSummary,
  ProgressEvent,
  RecoveryPayload,
  ReplicaAutomationStatus,
  ReviewAction,
  ReviewDecision,
  ReviewItem,
  Step,
  SupportBundlePreview,
  ThemePreference,
} from "./types";

type Route =
  | { name: "home" }
  | {
      name: "operation";
      operation: OperationDef;
      initialValues?: Record<string, unknown>;
      initialPlan?: Plan;
    }
  | { name: "review"; destination: PathSelection }
  | { name: "recovery" }
  | { name: "history" }
  | { name: "admin" }
  | { name: "settings"; back: "home" | "login" | "pending" };

const usernamePattern = /^[a-zA-Z0-9._-]{3,24}$/;

function isPathSelection(value: unknown): value is PathSelection {
  return Boolean(
    value &&
    typeof value === "object" &&
    "selectionId" in value &&
    typeof value.selectionId === "string" &&
    "displayPath" in value &&
    typeof value.displayPath === "string",
  );
}

export function humanError(error: unknown, s: Strings): string {
  const raw = error instanceof Error ? error.message : String(error);
  const localizedKey = raw.match(
    /^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+)$/,
  )?.[1];
  if (localizedKey) {
    const localized = s.t(localizedKey);
    if (localized !== localizedKey) return localized;
  }
  const detailedMatch = raw.match(
    /^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+):\s*(.+)$/,
  );
  if (detailedMatch) {
    const key = detailedMatch[1];
    const details = detailedMatch[2];
    const localized = s.t(key);
    if (localized !== key) {
      return `${localized} (${details})`;
    }
  }
  if (/error\.preset\.nameInvalid|preset[_ .-]?name[_ .-]?invalid/i.test(raw))
    return s.t("error.preset.nameInvalid");
  if (
    /error\.preset\.duplicate|duplicate[_ .-]?preset|preset.*already exists/i.test(
      raw,
    )
  )
    return s.t("error.preset.duplicate");
  if (/error\.preset\.limit|preset[_ .-]?(?:capacity|limit)/i.test(raw))
    return s.t("error.preset.limit");
  if (
    /error\.preset\.stale|preset[_ .-]?(?:missing|stale|not found)/i.test(raw)
  )
    return s.t("error.preset.stale");
  if (/error\.preset\.write|preset.*(?:write|persist|save).*fail/i.test(raw))
    return s.t("error.preset.write");
  if (/^(?:Error:\s*)?error\.rfi\.workbookInvalid$/.test(raw))
    return s.t("error.rfi.workbookInvalid");
  if (/^(?:Error:\s*)?error\.print\.queueSourceReselectionRequired$/.test(raw))
    return s.t("error.print.queueSourceReselectionRequired");
  if (/email_required/i.test(raw)) return s.t("auth.emailSignInHelp");
  if (/invalid_credentials|Invalid login/i.test(raw))
    return s.t("auth.err.credentials");
  if (/email_not_confirmed|Email not confirmed/i.test(raw))
    return s.t("auth.err.emailNotConfirmed");
  if (/rate.limit|too many requests/i.test(raw))
    return s.t("auth.err.rateLimit");
  if (/username_taken/i.test(raw)) return s.t("auth.err.usernameTaken");
  if (/username_invalid/i.test(raw)) return s.t("auth.err.usernameInvalid");
  if (/User already registered|user_already_exists/i.test(raw))
    return s.t("auth.err.emailTaken");
  if (/weak_password|Password should be/i.test(raw))
    return s.t("auth.err.weakPassword");
  return raw.replace(/^Error:\s*/, "");
}

function isDuplicatePresetError(error: unknown): boolean {
  const raw = error instanceof Error ? error.message : String(error);
  return /error\.preset\.duplicate|duplicate[_ .-]?preset|preset.*already exists/i.test(
    raw,
  );
}

function applyAppearance(prefs: AppPrefs): void {
  const root = document.documentElement;
  root.dataset.theme = prefs.theme;
  root.dataset.font = prefs.font;
  root.dataset.compact = String(prefs.compact);
  root.style.fontSize = `${prefs.fontScale * 16}px`;
}

export function App() {
  const bridge = useMemo(() => getBridge(), []);
  const initialStrings = useMemo(() => makeStrings(null), []);
  const [bootstrap, setBootstrap] = useState<Bootstrap | null>(null);
  const [fatal, setFatal] = useState("");
  const [route, setRoute] = useState<Route>({ name: "home" });

  const load = useCallback(async () => {
    setFatal("");
    try {
      const value = await bridge.call("app/bootstrap");
      setBootstrap(value);
      applyAppearance(value.prefs);
    } catch (error) {
      setFatal(error instanceof Error ? error.message : String(error));
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load]);

  if (!bootstrap) {
    return fatal ? (
      <Centered>
        <EmptyState
          icon={<XCircle />}
          title={initialStrings.t("bootstrap.fatalTitle")}
          message={fatal}
          action={
            <Button onClick={() => void load()} icon={<RefreshCw />}>
              {initialStrings.t("bootstrap.retry")}
            </Button>
          }
        />
      </Centered>
    ) : (
      <Splash />
    );
  }

  const s = makeStrings(bootstrap.prefs.locale);
  const updateAuth = (auth: AuthState) =>
    setBootstrap((current) => (current ? { ...current, auth } : current));
  const updatePrefs = (prefs: AppPrefs) => {
    applyAppearance(prefs);
    setBootstrap((current) => (current ? { ...current, prefs } : current));
  };

  if (route.name === "settings") {
    return (
      <SettingsScreen
        bridge={bridge}
        auth={bootstrap.auth}
        prefs={bootstrap.prefs}
        s={s}
        onPrefs={updatePrefs}
        onAuth={updateAuth}
        onBack={() => setRoute({ name: "home" })}
      />
    );
  }

  if (!bootstrap.auth.signedIn) {
    return (
      <LoginScreen
        bridge={bridge}
        s={s}
        onAuth={updateAuth}
        onSettings={() => setRoute({ name: "settings", back: "login" })}
      />
    );
  }

  if (!bootstrap.auth.allowed) {
    return (
      <PendingScreen
        bridge={bridge}
        auth={bootstrap.auth}
        s={s}
        onAuth={updateAuth}
        onSettings={() => setRoute({ name: "settings", back: "pending" })}
      />
    );
  }

  switch (route.name) {
    case "operation":
      return (
        <OperationScreen
          key={route.operation.id}
          bridge={bridge}
          operation={route.operation}
          initialValues={route.initialValues}
          initialPlan={route.initialPlan}
          s={s}
          onBack={() => setRoute({ name: "home" })}
          onReview={(destination) => setRoute({ name: "review", destination })}
          onPrepareRfiReply={(preparedPlan) => {
            const operation = bootstrap.operations.find(
              (candidate) => candidate.id === "rfi.outlook_reply_draft",
            );
            if (operation) {
              setRoute({
                name: "operation",
                operation,
                initialPlan: preparedPlan,
              });
            }
          }}
        />
      );
    case "review":
      return (
        <ReviewScreen
          bridge={bridge}
          destination={route.destination}
          s={s}
          onBack={() => setRoute({ name: "home" })}
        />
      );
    case "history":
      return (
        <HistoryScreen
          bridge={bridge}
          s={s}
          onBack={() => setRoute({ name: "home" })}
        />
      );
    case "recovery":
      return (
        <RecoveryScreen
          bridge={bridge}
          operations={bootstrap.operations}
          s={s}
          onBack={() => setRoute({ name: "home" })}
          onOpenOperation={(operation) =>
            setRoute({ name: "operation", operation })
          }
        />
      );
    case "admin":
      return (
        <AdminScreen
          bridge={bridge}
          auth={bootstrap.auth}
          s={s}
          onBack={() => setRoute({ name: "home" })}
        />
      );
    default:
      return (
        <HomeScreen
          bridge={bridge}
          bootstrap={bootstrap}
          s={s}
          onAuth={updateAuth}
          onPrefs={updatePrefs}
          navigate={setRoute}
        />
      );
  }
}

function Splash() {
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

function LoginScreen({
  bridge,
  s,
  onAuth,
  onSettings,
}: {
  bridge: Bridge;
  s: Strings;
  onAuth: (auth: AuthState) => void;
  onSettings: () => void;
}) {
  const [mode, setMode] = useState<"in" | "up">("in");
  const [identifier, setIdentifier] = useState("");
  const [email, setEmail] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{
    kind: "error" | "info";
    text: string;
  } | null>(null);

  useEffect(() => {
    setAvailable(null);
    if (mode !== "up" || !usernamePattern.test(username)) return;
    const timer = window.setTimeout(async () => {
      setChecking(true);
      try {
        setAvailable(await bridge.call("auth/usernameAvailable", { username }));
      } catch {
        setAvailable(null);
      } finally {
        setChecking(false);
      }
    }, 450);
    return () => window.clearTimeout(timer);
  }, [bridge, mode, username]);

  const valid =
    password.length >= 6 &&
    (mode === "in"
      ? identifier.trim().includes("@")
      : email.includes("@") &&
        usernamePattern.test(username) &&
        available !== false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    setMessage(null);
    try {
      if (mode === "in") {
        const auth = await bridge.call("auth/signIn", {
          identifier: identifier.trim(),
          password,
        });
        onAuth(auth);
      } else {
        await bridge.call("auth/signUp", {
          email: email.trim(),
          username: username.trim(),
          password,
        });
        setMessage({ kind: "info", text: s.t("auth.signupOk") });
      }
    } catch (error) {
      setMessage({ kind: "error", text: humanError(error, s) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="screen">
      <Header
        trailing={
          <IconButton label={s.t("prefs.title")} onClick={onSettings}>
            <Settings />
          </IconButton>
        }
      />
      <main className="login-wrap">
        <form className="login-card" onSubmit={(event) => void submit(event)}>
          <div className="brand-mark large">
            <Files />
          </div>
          <h1>{s.t("app.title")}</h1>
          <p className="muted center">
            {s.t(mode === "in" ? "auth.subtitle" : "auth.subtitleUp")}
          </p>
          <Segmented
            value={mode}
            options={[
              ["in", s.t("auth.signIn")],
              ["up", s.t("auth.signUp")],
            ]}
            onChange={(value) => {
              setMode(value as "in" | "up");
              setMessage(null);
            }}
          />
          <div className="form-stack">
            {mode === "in" ? (
              <Field
                autoFocus
                type="email"
                label={s.t("auth.email")}
                help={s.t("auth.emailSignInHelp")}
                autoComplete="email"
                value={identifier}
                onChange={setIdentifier}
                icon={<User />}
              />
            ) : (
              <>
                <Field
                  autoFocus
                  type="email"
                  autoComplete="email"
                  label={s.t("auth.email")}
                  value={email}
                  onChange={setEmail}
                  icon={<User />}
                />
                <Field
                  label={s.t("auth.username")}
                  autoComplete="username"
                  help={
                    available === false
                      ? s.t("auth.err.usernameTaken")
                      : s.t("auth.usernameHelp")
                  }
                  invalid={available === false}
                  value={username}
                  onChange={setUsername}
                  icon={<BadgeCheck />}
                  suffix={
                    checking ? (
                      <LoaderCircle className="spin" />
                    ) : available === true ? (
                      <Check />
                    ) : null
                  }
                />
              </>
            )}
            <Field
              type={showPassword ? "text" : "password"}
              autoComplete={mode === "in" ? "current-password" : "new-password"}
              label={s.t("auth.password")}
              help={s.t("auth.passwordHint")}
              value={password}
              onChange={setPassword}
              icon={<Shield />}
              suffix={
                <button
                  className="field-action"
                  type="button"
                  title={s.t(showPassword ? "auth.hide" : "auth.show")}
                  onClick={() => setShowPassword((value) => !value)}
                >
                  {showPassword ? <EyeOff /> : <Eye />}
                </button>
              }
            />
          </div>
          {message && <Notice kind={message.kind}>{message.text}</Notice>}
          <Button
            type="submit"
            disabled={!valid || busy}
            wide
            icon={busy ? <LoaderCircle className="spin" /> : <ArrowRight />}
          >
            {s.t(mode === "in" ? "auth.signIn" : "auth.signUp")}
          </Button>
        </form>
      </main>
    </div>
  );
}

function PendingScreen({
  bridge,
  auth,
  s,
  onAuth,
  onSettings,
}: {
  bridge: Bridge;
  auth: AuthState;
  s: Strings;
  onAuth: (auth: AuthState) => void;
  onSettings: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const run = async (request: () => Promise<AuthState>) => {
    setBusy(true);
    setError("");
    try {
      onAuth(await request());
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="screen">
      <Header
        title={s.t("app.title")}
        trailing={
          <>
            <IconButton label={s.t("prefs.title")} onClick={onSettings}>
              <Settings />
            </IconButton>
            <IconButton
              label={s.t("auth.signOut")}
              onClick={() => void run(() => bridge.call("auth/signOut"))}
            >
              <LogOut />
            </IconButton>
          </>
        }
      />
      <Centered>
        <EmptyState
          icon={<Shield />}
          title={s.t("auth.pending")}
          message={`${s.t("auth.pendingHelp")}\n${auth.session?.email ?? ""}`}
          action={
            <Button
              onClick={() => void run(() => bridge.call("auth/loadMe"))}
              disabled={busy}
              icon={<RefreshCw className={busy ? "spin" : ""} />}
            >
              {s.t("action.retry")}
            </Button>
          }
        />
        {error && <Notice kind="error">{error}</Notice>}
      </Centered>
    </div>
  );
}

function HomeScreen({
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
  const who =
    bootstrap.auth.user?.display || bootstrap.auth.session?.email || "";
  const [reviewBusy, setReviewBusy] = useState(false);
  const [shortcutError, setShortcutError] = useState("");
  const signOut = async () => onAuth(await bridge.call("auth/signOut"));

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

  const openReview = async () => {
    setReviewBusy(true);
    setShortcutError("");
    try {
      const destination = await chooseFolder(
        bridge,
        "rfi.review",
        "destination",
      );
      if (destination) navigate({ name: "review", destination });
    } catch (reason) {
      setShortcutError(humanError(reason, s));
    } finally {
      setReviewBusy(false);
    }
  };
  return (
    <div className="screen">
      <Header
        title={s.t("app.title")}
        trailing={
          <>
            {who && (
              <span className="user-chip">
                <User /> {who}
              </span>
            )}
            {bootstrap.auth.isAdmin && (
              <IconButton
                label={s.t("action.admin")}
                onClick={() => navigate({ name: "admin" })}
              >
                <Shield />
              </IconButton>
            )}
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
              label={s.t("action.history")}
              onClick={() => navigate({ name: "history" })}
            >
              <History />
            </IconButton>
            <IconButton
              label={s.t("prefs.title")}
              onClick={() => navigate({ name: "settings", back: "home" })}
            >
              <Settings />
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
        <div className="dashboard-grid">
          <section className="dashboard-section">
            <div className="section-heading">
              <div>
                <h2>{s.t("ops.heading")}</h2>
                <p>{s.t("ops.subheading")}</p>
              </div>
              <span className="section-count">
                {bootstrap.operations.length}
              </span>
            </div>
            <div className="operation-list">
              {bootstrap.operations.map((operation) => (
                <button
                  className="operation-card"
                  key={operation.id}
                  onClick={() => navigate({ name: "operation", operation })}
                >
                  <span className="operation-icon">
                    <FileSearch />
                  </span>
                  <span className="operation-copy">
                    <strong>{s.t(operation.titleKey)}</strong>
                    <small>{s.t(operation.descriptionKey)}</small>
                  </span>
                  <ChevronRight />
                </button>
              ))}
            </div>
          </section>
          <section className="dashboard-section">
            <div className="section-heading">
              <div>
                <h2>{s.t("home.features")}</h2>
                <p>{s.t("home.features.help")}</p>
              </div>
            </div>
            <div className="feature-list">
              <button
                className="feature-card"
                disabled={reviewBusy}
                onClick={() => void openReview()}
              >
                <span className="feature-icon">
                  {reviewBusy ? (
                    <LoaderCircle className="spin" />
                  ) : (
                    <FileCheck2 />
                  )}
                </span>
                <span className="feature-copy">
                  <strong>{s.t("action.review")}</strong>
                  <small>{s.t("home.review.desc")}</small>
                </span>
                <ChevronRight />
              </button>
              <button
                className="feature-card"
                onClick={() => navigate({ name: "history" })}
              >
                <span className="feature-icon">
                  <History />
                </span>
                <span className="feature-copy">
                  <strong>{s.t("action.history")}</strong>
                  <small>{s.t("home.history.desc")}</small>
                </span>
                <ChevronRight />
              </button>
              <button
                className="feature-card"
                onClick={() => navigate({ name: "recovery" })}
              >
                <span className="feature-icon">
                  <CircleAlert />
                </span>
                <span className="feature-copy">
                  <strong>{s.t("recovery.title")}</strong>
                  <small>{s.t("home.recovery.desc")}</small>
                </span>
                <ChevronRight />
              </button>
              <button
                className="feature-card"
                onClick={() => navigate({ name: "settings", back: "home" })}
              >
                <span className="feature-icon">
                  <Settings />
                </span>
                <span className="feature-copy">
                  <strong>{s.t("prefs.title")}</strong>
                  <small>{s.t("home.settings.desc")}</small>
                </span>
                <ChevronRight />
              </button>
              {bootstrap.auth.isAdmin && (
                <button
                  className="feature-card"
                  onClick={() => navigate({ name: "admin" })}
                >
                  <span className="feature-icon">
                    <UserRoundCog />
                  </span>
                  <span className="feature-copy">
                    <strong>{s.t("action.admin")}</strong>
                    <small>{s.t("home.admin.desc")}</small>
                  </span>
                  <ChevronRight />
                </button>
              )}
            </div>
          </section>
        </div>
        {shortcutError && <Notice kind="error">{shortcutError}</Notice>}
      </main>
    </div>
  );
}

function OperationScreen({
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
          Object.fromEntries(
            Object.entries({
              ...defaults,
              ...filtered,
              ...(initialValues ?? {}),
            }).filter(([name]) => {
              const param = operation.schema.find((item) => item.name === name);
              return (
                param?.remember !== false &&
                param?.kind !== "longText" &&
                param?.kind !== "secret"
              );
            }),
          ),
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
    const sanitized = Object.fromEntries(
      Object.entries(values).filter(([name]) => {
        const param = operation.schema.find((item) => item.name === name);
        return (
          param?.remember !== false &&
          param?.kind !== "longText" &&
          param?.kind !== "secret"
        );
      }),
    );
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
    setStage("preview");
  };

  const preview = async () => {
    setError("");
    const preparing = { phase: "preparing", done: 0, total: 0 };
    setProgress(preparing);
    setProgressTrail([preparing]);
    setStage("previewing");
    try {
      const sanitized = Object.fromEntries(
        Object.entries(values).filter(([name]) => {
          const param = operation.schema.find((item) => item.name === name);
          return (
            param?.remember !== false &&
            param?.kind !== "longText" &&
            param?.kind !== "secret"
          );
        }),
      );
      lastSaved.current = JSON.stringify(sanitized);
      await bridge.call("settings/saveParams", {
        opId: operation.id,
        values: sanitized,
      });
      if (operation.supportsPresets && Object.keys(sanitized).length > 0) {
        setPreviousValues(sanitized);
      }
      const next = await bridge.call(
        "engine/preview",
        { opId: operation.id, values },
        receiveProgress,
      );
      installPlan(next);
    } catch (reason) {
      setError(humanError(reason, s));
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
  const apply = async () => {
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
        if (stage === "preview" || stage === "result") {
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
  }, [stage, cancelling, preview, apply, onBack]);

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
                <div className="form-stack">
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
                        }}
                      />
                    ))}
                </div>
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
          <BusyState
            label={s.t("action.preview")}
            progress={progress}
            progressTrail={progressTrail}
            s={s}
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
          />
        )}
        {stage === "applying" && (
          <div className="flow-layout">
            <BusyState
              label={s.t("plan.applying")}
              progress={progress}
              progressTrail={progressTrail}
              s={s}
            />
            <ActionBar
              left={
                <Button
                  variant="danger"
                  icon={cancelling ? <LoaderCircle className="spin" /> : <X />}
                  disabled={cancelling}
                  onClick={() => void cancelApply()}
                >
                  {s.t(
                    cancelling
                      ? "operation.cancelling"
                      : operation.id === "files.batch_print"
                        ? "print.cancelQueue"
                        : "operation.cancel",
                  )}
                </Button>
              }
              right={null}
            />
          </div>
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
          />
        )}
      </main>
    </div>
  );
}

type PresetToolbarProps = {
  bridge: Bridge;
  operation: OperationDef;
  values: Record<string, unknown>;
  s: Strings;
  onApply: (preset: OperationPreset) => void;
  onError: (message: string) => void;
};

function presetCapableValues(
  operation: OperationDef,
  values: Record<string, unknown>,
): Record<string, unknown> {
  const allowed = new Set(
    operation.schema
      .filter(
        (param) =>
          param.kind === "bool" ||
          param.kind === "choice" ||
          param.kind === "int",
      )
      .map((param) => param.name),
  );
  return Object.fromEntries(
    Object.entries(values).filter(([name]) => allowed.has(name)),
  );
}

function presetCommandError(error: unknown, s: Strings): string {
  if (
    error &&
    typeof error === "object" &&
    "code" in error &&
    error.code === "settings_write_failed"
  ) {
    return s.t("error.preset.write");
  }
  return humanError(error, s);
}

function PresetToolbar({
  bridge,
  operation,
  values,
  s,
  onApply,
  onError,
}: PresetToolbarProps) {
  const [presets, setPresets] = useState<OperationPreset[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  const loadPresets = useCallback(async () => {
    setLoading(true);
    try {
      const next = await bridge.call("settings/presets", {
        opId: operation.id,
      });
      setPresets(next);
      return next;
    } catch (reason) {
      onError(presetCommandError(reason, s));
      return null;
    } finally {
      setLoading(false);
    }
  }, [bridge, onError, operation.id, s]);

  useEffect(() => {
    if (!operation.supportsPresets) return;
    void loadPresets();
  }, [loadPresets, operation.supportsPresets]);

  const selected = presets.find((preset) => preset.id === selectedId);
  const applyPreset = () => {
    if (!selected) return;
    onApply({
      ...selected,
      values: presetCapableValues(operation, selected.values),
    });
  };
  const savePreset = async () => {
    const trimmedName = name.trim();
    if (!trimmedName || busy) return;
    setBusy(true);
    setStatus("");
    onError("");
    try {
      const presetValues = presetCapableValues(operation, values);
      let saved: OperationPreset;
      try {
        saved = await bridge.call("settings/savePreset", {
          opId: operation.id,
          name: trimmedName,
          values: presetValues,
        });
      } catch (reason) {
        if (
          !isDuplicatePresetError(reason) ||
          !window.confirm(s.t("preset.confirm.replace", { name: trimmedName }))
        ) {
          throw reason;
        }
        saved = await bridge.call("settings/savePreset", {
          opId: operation.id,
          name: trimmedName,
          values: presetValues,
          replace: true,
        });
      }
      const next = await loadPresets();
      if (next) {
        setSelectedId(saved.id);
        setName("");
        setStatus(s.t("preset.saved", { name: saved.name }));
      }
    } catch (reason) {
      onError(presetCommandError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  const deletePreset = async () => {
    if (
      !selected ||
      busy ||
      !window.confirm(s.t("preset.confirm.delete", { name: selected.name }))
    ) {
      return;
    }
    setBusy(true);
    setStatus("");
    onError("");
    try {
      await bridge.call("settings/deletePreset", {
        opId: operation.id,
        presetId: selected.id,
      });
      const deletedName = selected.name;
      const next = await loadPresets();
      if (next) {
        setSelectedId("");
        setStatus(s.t("preset.deleted", { name: deletedName }));
      }
    } catch (reason) {
      onError(presetCommandError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="preset-toolbar" aria-labelledby="preset-title">
      <div className="preset-toolbar-heading">
        <h3 id="preset-title">{s.t("preset.title")}</h3>
        {!loading && presets.length === 0 && (
          <small>{s.t("preset.empty")}</small>
        )}
      </div>
      <div className="preset-toolbar-row">
        <label className="field preset-select">
          <span>{s.t("preset.select")}</span>
          <select
            value={selectedId}
            disabled={loading || busy}
            onChange={(event) => setSelectedId(event.target.value)}
          >
            <option value="">{s.t("preset.select.placeholder")}</option>
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
        </label>
        <Button
          type="button"
          variant="secondary"
          disabled={!selected || busy}
          onClick={applyPreset}
        >
          {s.t("preset.apply")}
        </Button>
        <Button
          type="button"
          variant="danger"
          icon={<Trash2 />}
          disabled={!selected || busy}
          onClick={deletePreset}
        >
          {s.t("preset.delete")}
        </Button>
      </div>
      <div className="preset-toolbar-row preset-save-row">
        <label className="field preset-name">
          <span>{s.t("preset.name")}</span>
          <input
            type="text"
            value={name}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <Button
          type="button"
          icon={busy ? <LoaderCircle className="spin" /> : <Save />}
          disabled={!name.trim() || busy}
          onClick={savePreset}
        >
          {s.t("preset.save")}
        </Button>
      </div>
      {status && <small role="status">{status}</small>}
    </section>
  );
}

function SchemaField({
  bridge,
  operationId,
  param,
  value,
  values,
  s,
  onChange,
}: {
  bridge: Bridge;
  operationId: string;
  param: ParamDef;
  value: unknown;
  values: Record<string, unknown>;
  s: Strings;
  onChange: (value: unknown) => void;
}) {
  if (
    !Object.entries(param.visibleWhen ?? {}).every(
      ([name, expected]) => values[name] === expected,
    )
  ) {
    return null;
  }
  if (param.kind === "bool") {
    return (
      <label className="toggle-row">
        <span>
          <strong>{s.t(param.labelKey)}</strong>
          {param.helpKey && <small>{s.t(param.helpKey)}</small>}
        </span>
        <Switch checked={Boolean(value)} onChange={onChange} />
      </label>
    );
  }
  if (param.kind === "choice") {
    const currentChoice = String(value ?? "");
    const choiceIsTemporarilyUnavailable =
      currentChoice.length > 0 && !param.choices.includes(currentChoice);
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <select
          value={currentChoice}
          onChange={(event) => onChange(event.target.value)}
        >
          {choiceIsTemporarilyUnavailable && (
            <option value={currentChoice}>{currentChoice}</option>
          )}
          {param.choices.map((choice) => (
            <option key={choice} value={choice}>
              {param.choiceLabels?.[choice]
                ? s.t(param.choiceLabels[choice])
                : choice}
            </option>
          ))}
        </select>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  if (param.kind === "files") {
    const paths = Array.isArray(value) ? value.filter(isPathSelection) : [];
    const [dragOver, setDragOver] = useState(false);
    return (
      <div
        className={`field dropzone-field ${dragOver ? "drop-active" : ""}`}
        onDragOver={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(true);
        }}
        onDragLeave={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(false);
        }}
        onDrop={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(false);
          const dropped = Array.from(event.dataTransfer.files).map(
            (file) => (file as unknown as { path?: string }).path || file.name,
          );
          if (dropped.length > 0) {
            void dropPaths(bridge, operationId, param.name, dropped).then(
              (selected) => {
                const merged = new Map(
                  [...paths, ...selected].map((item) => [
                    item.selectionId,
                    item,
                  ]),
                );
                onChange([...merged.values()]);
              },
            );
          }
        }}
      >
        <span>{s.t(param.labelKey)}</span>
        <div className="multi-file-actions">
          <Button
            type="button"
            variant="secondary"
            icon={<Plus />}
            onClick={() =>
              void chooseFiles(bridge, operationId, param.name).then(
                (selected) => {
                  const merged = new Map(
                    [...paths, ...selected].map((item) => [
                      item.selectionId,
                      item,
                    ]),
                  );
                  onChange([...merged.values()]);
                },
              )
            }
          >
            {s.t("action.addFiles")}
          </Button>
          {paths.length > 0 && (
            <Button type="button" variant="ghost" onClick={() => onChange([])}>
              {s.t("action.clear")}
            </Button>
          )}
        </div>
        {paths.length > 0 && (
          <div className="selected-files">
            {paths.map((path, index) => (
              <div className="selected-file" key={path.selectionId}>
                <span title={path.displayPath}>{path.name}</span>
                <button
                  type="button"
                  title={s.t("action.remove")}
                  onClick={() =>
                    onChange(
                      paths.filter((_, itemIndex) => itemIndex !== index),
                    )
                  }
                >
                  <X />
                </button>
              </div>
            ))}
          </div>
        )}
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </div>
    );
  }
  if (param.kind === "sheetCounts") {
    const counts =
      value && typeof value === "object"
        ? (value as Record<string, number>)
        : {};
    const update = (choice: string, next: number) =>
      onChange({ ...counts, [choice]: Math.max(0, Math.min(100, next)) });
    const total = param.choices.reduce(
      (sum, choice) => sum + Number(counts[choice] ?? 0),
      0,
    );
    return (
      <div className="field sheet-count-field">
        <span>{s.t(param.labelKey)}</span>
        <div className="sheet-count-grid">
          {param.choices.map((choice) => {
            const count = Number(counts[choice] ?? 0);
            return (
              <div
                className={`sheet-count ${count > 0 ? "active" : ""}`}
                key={choice}
              >
                <strong>{choice}</strong>
                <div>
                  <button
                    type="button"
                    disabled={count <= 0}
                    aria-label={`${choice} ${s.t("action.decrease")}`}
                    onClick={() => update(choice, count - 1)}
                  >
                    −
                  </button>
                  <span>{count}</span>
                  <button
                    type="button"
                    disabled={count >= 100}
                    aria-label={`${choice} ${s.t("action.increase")}`}
                    onClick={() => update(choice, count + 1)}
                  >
                    +
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <small>{s.t("rfi.sheetCounts.total", { count: total })}</small>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </div>
    );
  }
  const browse = param.kind === "folder" || param.kind === "file";
  const pathValue = isPathSelection(value) ? value.displayPath : "";
  const [browseDragOver, setBrowseDragOver] = useState(false);
  if (param.kind === "longText") {
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <textarea
          rows={8}
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  return (
    <label className="field">
      <span>{s.t(param.labelKey)}</span>
      <div
        className={`path-input ${browse ? "dropzone-field" : ""} ${browse && browseDragOver ? "drop-active" : ""}`}
        onDragOver={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(true);
              }
            : undefined
        }
        onDragLeave={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(false);
              }
            : undefined
        }
        onDrop={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(false);
                const dropped = Array.from(event.dataTransfer.files).map(
                  (file) =>
                    (file as unknown as { path?: string }).path || file.name,
                );
                if (dropped.length > 0) {
                  void dropPaths(bridge, operationId, param.name, dropped).then(
                    (selected) => {
                      if (selected[0]) onChange(selected[0]);
                    },
                  );
                }
              }
            : undefined
        }
      >
        <input
          type={
            param.kind === "int"
              ? "number"
              : param.kind === "secret"
                ? "password"
                : "text"
          }
          value={browse ? pathValue : String(value ?? "")}
          placeholder={browse ? s.t(`param.hint.${param.kind}`) : ""}
          readOnly={browse}
          onChange={(event) =>
            !browse &&
            onChange(
              param.kind === "int"
                ? Number(event.target.value)
                : event.target.value,
            )
          }
        />
        {browse && (
          <Button
            type="button"
            variant="secondary"
            icon={<FolderOpen />}
            onClick={() =>
              void (
                param.kind === "folder"
                  ? chooseFolder(bridge, operationId, param.name)
                  : chooseFile(bridge, operationId, param.name)
              ).then((path) => {
                if (path) onChange(path);
              })
            }
          >
            {s.t("action.browse")}
          </Button>
        )}
        {browse && Boolean(pathValue) && (
          <button
            type="button"
            className="field-action"
            title={s.t("action.clear")}
            aria-label={s.t("action.clear")}
            onClick={() => onChange(null)}
          >
            <X />
          </button>
        )}
      </div>
      {param.helpKey && <small>{s.t(param.helpKey)}</small>}
    </label>
  );
}

function copySheetsFromPlan(plan: Plan | null): string[] {
  const editor = plan?.meta?.editor;
  if (!editor || typeof editor !== "object") return [];
  const sources = (editor as Record<string, unknown>).copySources;
  return sources && typeof sources === "object"
    ? Object.keys(sources as Record<string, unknown>)
    : [];
}

type EditorColumn = {
  key: string;
  labelKey: string;
  kind?: string;
  width?: number;
  sheets?: string[];
  detail?: boolean;
  bulk?: boolean;
  display?: string;
  disabledWhen?: { field: string; equals: string | number | boolean };
};

const EDITOR_DATE_MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;

type EditorDateParts = {
  day: number;
  month: number;
  year: number;
};

function parseEditorDate(value: string): EditorDateParts | null {
  const match = value.trim().match(/^(\d{1,2})-([A-Za-z]{3})-(\d{2}|\d{4})$/);
  if (!match) return null;
  const day = Number(match[1]);
  const month =
    EDITOR_DATE_MONTHS.findIndex(
      (name) => name.toLowerCase() === match[2].toLowerCase(),
    ) + 1;
  const rawYear = Number(match[3]);
  const year = rawYear < 100 ? 2000 + rawYear : rawYear;
  if (!month || year < 2000 || year > 2068) return null;
  const maxDay = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return day >= 1 && day <= maxDay ? { day, month, year } : null;
}

function formatEditorDate({ day, month, year }: EditorDateParts): string {
  return `${String(day).padStart(2, "0")}-${EDITOR_DATE_MONTHS[month - 1]}-${String(year).slice(-2)}`;
}

function displayEditorValue(column: EditorColumn, value: string): string {
  if (column.display === "suffix") {
    return value.match(/(\d+)\s*$/)?.[1] ?? value;
  }
  if (column.display === "shortNumber") {
    const suffix = value.match(/(\d+)\s*$/)?.[1];
    return suffix ? String(Number(suffix)) : value;
  }
  if (column.display === "itemSuffix") {
    return value.replace(/^[A-Z]{2,5}[-\u02d7\u2010-\u2015]+/i, "");
  }
  if (column.display === "percent") {
    return formatConfidencePercent(value, "fraction") ?? "—";
  }
  return value;
}

export function formatConfidencePercent(
  value: unknown,
  scale: "fraction" | "percent" = "fraction",
): string | null {
  const numeric = typeof value === "number" ? value : Number(value);
  if (
    (typeof value === "string" && value.trim() === "") ||
    !Number.isFinite(numeric)
  ) {
    return null;
  }
  const percent = scale === "fraction" ? numeric * 100 : numeric;
  return `${Math.round(Math.min(100, Math.max(0, percent)))}%`;
}

function CompactDateInput({
  value,
  label,
  s,
  onChange,
}: {
  value: string;
  label: string;
  s: Strings;
  onChange: (value: string) => void;
}) {
  const currentYear = new Date().getFullYear();
  const parsed = parseEditorDate(value);
  const [draft, setDraft] = useState<{
    day: number | "";
    month: number | "";
    year: number;
  }>(() => parsed ?? { day: "", month: "", year: currentYear });
  const [yearVisible, setYearVisible] = useState(false);

  useEffect(() => {
    const next = parseEditorDate(value);
    setDraft(next ?? { day: "", month: "", year: currentYear });
  }, [value, currentYear]);

  const commit = (next: typeof draft) => {
    let normalized = next;
    if (next.day && next.month) {
      const maxDay = new Date(Date.UTC(next.year, next.month, 0)).getUTCDate();
      normalized = { ...next, day: Math.min(next.day, maxDay) };
      onChange(formatEditorDate(normalized as EditorDateParts));
    }
    setDraft(normalized);
  };
  const maxDay = draft.month
    ? new Date(Date.UTC(draft.year, draft.month, 0)).getUTCDate()
    : 31;
  const monthLabels = useMemo(
    () =>
      EDITOR_DATE_MONTHS.map((_, index) =>
        new Intl.DateTimeFormat(s.locale, { month: "short" }).format(
          new Date(Date.UTC(2024, index, 1)),
        ),
      ),
    [s.locale],
  );

  return (
    <div className="compact-date-cell" title={value || undefined}>
      <select
        className="compact-date-day"
        aria-label={`${label} — ${s.t("rfi.date.day")}`}
        value={draft.day}
        onChange={(event) =>
          commit({ ...draft, day: Number(event.target.value) || "" })
        }
      >
        <option value="">--</option>
        {Array.from({ length: maxDay }, (_, index) => index + 1).map((day) => (
          <option key={day} value={day}>
            {String(day).padStart(2, "0")}
          </option>
        ))}
      </select>
      <select
        className="compact-date-month"
        aria-label={`${label} — ${s.t("rfi.date.month")}`}
        value={draft.month}
        onChange={(event) =>
          commit({ ...draft, month: Number(event.target.value) || "" })
        }
      >
        <option value="">---</option>
        {monthLabels.map((month, index) => (
          <option key={EDITOR_DATE_MONTHS[index]} value={index + 1}>
            {month.replace(".", "")}
          </option>
        ))}
      </select>
      <button
        type="button"
        className={yearVisible ? "active" : ""}
        aria-label={`${label} — ${s.t("rfi.date.changeYear")}`}
        title={s.t("rfi.date.changeYear")}
        onClick={() => setYearVisible((visible) => !visible)}
      >
        <CalendarDays />
      </button>
      {yearVisible && (
        <select
          autoFocus
          className="compact-date-year"
          aria-label={`${label} — ${s.t("rfi.date.year")}`}
          value={draft.year}
          onBlur={() => setYearVisible(false)}
          onChange={(event) => {
            commit({ ...draft, year: Number(event.target.value) });
            setYearVisible(false);
          }}
        >
          {Array.from({ length: 69 }, (_, index) => 2000 + index).map(
            (year) => (
              <option key={year} value={year}>
                {year}
              </option>
            ),
          )}
        </select>
      )}
    </div>
  );
}

function EditablePlanTable({
  editor,
  rows,
  s,
  onChange,
}: {
  editor: EditorMetadata;
  rows: Record<string, unknown>[];
  s: Strings;
  onChange: (rows: Record<string, unknown>[]) => void;
}) {
  const columns = Array.isArray(editor.columns)
    ? (editor.columns as EditorColumn[])
    : [];
  const groupField = String(editor.groupField ?? "rfiNo");
  const directChoices =
    editor.choices && typeof editor.choices === "object"
      ? (editor.choices as Record<string, unknown>)
      : {};
  const dependentChoices =
    editor.choicesByValue && typeof editor.choicesByValue === "object"
      ? (editor.choicesByValue as Record<string, unknown>)
      : {};
  const autocomplete =
    editor.autocomplete && typeof editor.autocomplete === "object"
      ? (editor.autocomplete as Record<string, unknown>)
      : {};
  const choiceLabels =
    editor.choiceLabels && typeof editor.choiceLabels === "object"
      ? (editor.choiceLabels as Record<string, unknown>)
      : {};
  const rowId = (row: Record<string, unknown>, index: number) =>
    String(row._rowId ?? `${row[groupField] ?? "row"}-${index}`);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [detailsVisible, setDetailsVisible] = useState(
    editor.detailsDefaultVisible === true,
  );
  const bulkColumns = columns.filter(
    (column) =>
      column.bulk !== false &&
      column.kind !== "readonly" &&
      (!column.detail || detailsVisible),
  );
  const [bulkField, setBulkField] = useState(bulkColumns[0]?.key ?? "");
  const [bulkValue, setBulkValue] = useState("");
  const visibleColumns = columns.filter(
    (column) => !column.detail || detailsVisible,
  );
  const selectedRows = rows.filter((row, index) =>
    selectedIds.has(rowId(row, index)),
  );

  const [searchTerm, setSearchTerm] = useState("");
  const [filterMode, setFilterMode] = useState<"all" | "changes" | "review">(
    "all",
  );

  const normalizedSearch = searchTerm.trim().toLowerCase();
  const filteredRowIndices = useMemo(() => {
    return rows
      .map((row, index) => {
        if (filterMode === "changes") {
          const isChange =
            (row.proposedStatus && row.proposedStatus !== row.currentStatus) ||
            row.enabled === true;
          if (!isChange) return -1;
        } else if (filterMode === "review") {
          const isReview =
            row.disposition === "manual_review" ||
            Boolean(row.reasonKey) ||
            row.selectable === false ||
            row.enabled === false;
          if (!isReview) return -1;
        }
        if (normalizedSearch) {
          const matches = Object.values(row).some((val) =>
            typeof val === "string" || typeof val === "number"
              ? String(val).toLowerCase().includes(normalizedSearch)
              : false,
          );
          if (!matches) return -1;
        }
        return index;
      })
      .filter((index) => index !== -1);
  }, [rows, filterMode, normalizedSearch]);

  useEffect(() => {
    const validIds = new Set(rows.map((row, index) => rowId(row, index)));
    setSelectedIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [rows, groupField]);

  useEffect(() => {
    if (bulkColumns.some((column) => column.key === bulkField)) return;
    setBulkField(bulkColumns[0]?.key ?? "");
    setBulkValue("");
  }, [bulkField, detailsVisible, editor.columns]);

  const choiceLabel = (field: string, value: string, column?: EditorColumn) => {
    const fieldLabels = choiceLabels[field];
    if (fieldLabels && typeof fieldLabels === "object") {
      const key = (fieldLabels as Record<string, unknown>)[value];
      if (typeof key === "string") return s.t(key);
    }
    return column ? displayEditorValue(column, value) : value;
  };

  const choicesFor = (
    column: EditorColumn,
    row: Record<string, unknown>,
  ): string[] => {
    const direct = directChoices[column.key];
    if (Array.isArray(direct)) return direct.map(String);
    const config = dependentChoices[column.key];
    if (!config || typeof config !== "object") return [];
    const typed = config as Record<string, unknown>;
    const values = typed.values;
    const dependency = String(typed.dependsOn ?? "");
    if (!values || typeof values !== "object") return [];
    const options = (values as Record<string, unknown>)[
      String(row[dependency] ?? "")
    ];
    return Array.isArray(options) ? options.map(String) : [];
  };

  const applyValue = (
    row: Record<string, unknown>,
    key: string,
    value: string | boolean,
  ) => {
    row[key] = value;

    const autoFill = editor.autoFill;
    if (autoFill && typeof autoFill === "object") {
      const config = autoFill as Record<string, unknown>;
      if (
        config.field === key &&
        config.values &&
        typeof config.values === "object"
      ) {
        const preset = (config.values as Record<string, unknown>)[
          String(value)
        ];
        if (preset && typeof preset === "object") Object.assign(row, preset);
      }
    }

    if (Array.isArray(editor.regexFill)) {
      for (const rawRule of editor.regexFill) {
        if (!rawRule || typeof rawRule !== "object") continue;
        const rule = rawRule as Record<string, unknown>;
        if (rule.source !== key) continue;
        try {
          if (new RegExp(String(rule.pattern), "i").test(String(value))) {
            row[String(rule.target)] = String(rule.value ?? "");
          }
        } catch {
          // Invalid server-side presentation hints never block row editing.
        }
      }
    }
  };

  const update = (index: number, key: string, value: string | boolean) => {
    const next = rows.map((row) => ({ ...row }));
    applyValue(next[index], key, value);
    onChange(next);
  };

  const applyBulk = () => {
    if (!bulkField || !selectedIds.size) return;
    const nextValue: string | boolean =
      bulkColumns.find((column) => column.key === bulkField)?.kind === "bool"
        ? bulkValue === "true"
        : bulkValue;
    const next = rows.map((row, index) => {
      const clone = { ...row };
      if (selectedIds.has(rowId(row, index))) {
        applyValue(clone, bulkField, nextValue);
      }
      return clone;
    });
    onChange(next);
  };

  const selectAll = () =>
    setSelectedIds(
      new Set(filteredRowIndices.map((index) => rowId(rows[index], index))),
    );
  const selectNone = () => setSelectedIds(new Set());
  const toggleRow = (id: string) =>
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const selectGroup = (field: string, value: unknown) =>
    setSelectedIds(
      new Set(
        rows
          .map((row, index) => ({ row, id: rowId(row, index) }))
          .filter(({ row }) => row[field] === value)
          .map(({ id }) => id),
      ),
    );

  const duplicate = (index: number) => {
    const source = rows[index];
    const group = source[groupField];
    let insertAt = index + 1;
    while (insertAt < rows.length && rows[insertAt][groupField] === group) {
      insertAt += 1;
    }
    const clone = {
      ...source,
      _rowId: `${String(source._rowId ?? group)}-copy-${Date.now()}`,
    };
    const next = [...rows];
    next.splice(insertAt, 0, clone);
    onChange(next);
  };

  const remove = (index: number) => {
    const group = rows[index][groupField];
    if (rows.filter((row) => row[groupField] === group).length <= 1) return;
    onChange(rows.filter((_, rowIndex) => rowIndex !== index));
  };

  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= rows.length) return;
    const next = [...rows];
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
  };

  const selectionGroupFields = Array.isArray(editor.selectionGroupFields)
    ? (editor.selectionGroupFields as unknown[]).map(String)
    : [];
  const groupChoices = selectionGroupFields.flatMap((field) => {
    const values = [
      ...new Set(rows.map((row) => String(row[field] ?? ""))),
    ].filter(Boolean);
    return values.length <= 12 ? values.map((value) => ({ field, value })) : [];
  });
  const bulkColumn = bulkColumns.find((column) => column.key === bulkField);
  const choiceRows = selectedRows.length
    ? selectedRows
    : rows.length
      ? [rows[0]]
      : [];
  const bulkChoiceValues = bulkColumn
    ? choiceRows
        .map((row) => choicesFor(bulkColumn, row))
        .reduce<string[]>(
          (common, values, index) =>
            index === 0
              ? values
              : common.filter((value) => values.includes(value)),
          [],
        )
    : [];
  const bulkSuggestions = Array.isArray(autocomplete[bulkField])
    ? (autocomplete[bulkField] as unknown[]).map(String)
    : [];
  const bulkListId = `bulk-${bulkField}`;

  return (
    <section
      className={`editor-card${editor.layout === "compact" ? " editor-card--compact" : ""}`}
    >
      <div className="editor-heading">
        <span>
          <TableProperties />
        </span>
        <div>
          <strong>{s.t(String(editor.titleKey ?? "rfi.editor.title"))}</strong>
          <small>
            {s.t(String(editor.rowCountKey ?? "rfi.editor.rowCount"), {
              count: rows.length,
            })}
          </small>
        </div>
        <div className="editor-heading-actions">
          {editor.detailsEnabled === true && (
            <Button
              variant="ghost"
              icon={detailsVisible ? <EyeOff /> : <Eye />}
              onClick={() => setDetailsVisible((visible) => !visible)}
            >
              {s.t(
                detailsVisible
                  ? "editor.action.hideDetails"
                  : "editor.action.showDetails",
              )}
            </Button>
          )}
        </div>
      </div>
      <div className="table-filter-bar">
        <div className="table-search-box">
          <input
            type="search"
            aria-label={s.t("filter.searchPlaceholder")}
            placeholder={s.t("filter.searchPlaceholder")}
            value={searchTerm}
            onChange={(event) => setSearchTerm(event.target.value)}
          />
        </div>
        <div className="table-filter-tabs">
          <button
            type="button"
            className={`filter-tab ${filterMode === "all" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("all")}
          >
            {s.t("filter.all")}
          </button>
          <button
            type="button"
            className={`filter-tab ${filterMode === "changes" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("changes")}
          >
            {s.t("filter.changes")}
          </button>
          <button
            type="button"
            className={`filter-tab ${filterMode === "review" ? "filter-tab--active" : ""}`}
            onClick={() => setFilterMode("review")}
          >
            {s.t("filter.manualReview")}
          </button>
        </div>
        {(searchTerm !== "" || filterMode !== "all") && (
          <span className="table-filter-count">
            {s.t("table.filterCount", {
              filtered: filteredRowIndices.length,
              total: rows.length,
            })}
          </span>
        )}
      </div>
      {editor.bulkEnabled === true && (
        <div className="bulk-editor" aria-label={s.t("editor.bulk.title")}>
          <div className="bulk-selection">
            <strong>
              {s.t("editor.selectedCount", { count: selectedIds.size })}
            </strong>
            <button type="button" onClick={selectAll}>
              {s.t("editor.selectAll")}
            </button>
            <button type="button" onClick={selectNone}>
              {s.t("editor.selectNone")}
            </button>
            {groupChoices.map(({ field, value }) => (
              <button
                type="button"
                key={`${field}-${value}`}
                title={
                  editor.layout === "compact" && field === groupField
                    ? value
                    : undefined
                }
                onClick={() => selectGroup(field, value)}
              >
                {editor.layout === "compact" && field === groupField
                  ? (value.match(/(\d+)\s*$/)?.[1] ?? value)
                  : value}
              </button>
            ))}
          </div>
          <div className="bulk-fields">
            <select
              aria-label={s.t("editor.bulk.field")}
              value={bulkField}
              onChange={(event) => {
                setBulkField(event.target.value);
                setBulkValue("");
              }}
            >
              {bulkColumns.map((column) => (
                <option key={column.key} value={column.key}>
                  {s.t(column.labelKey)}
                </option>
              ))}
            </select>
            {bulkColumn?.kind === "bool" ? (
              <select
                aria-label={s.t("editor.bulk.value")}
                value={bulkValue}
                onChange={(event) => setBulkValue(event.target.value)}
              >
                <option value="">{s.t("editor.bulk.choose")}</option>
                <option value="true">{s.t("editor.bool.true")}</option>
                <option value="false">{s.t("editor.bool.false")}</option>
              </select>
            ) : bulkColumn?.kind === "choice" ? (
              <select
                aria-label={s.t("editor.bulk.value")}
                value={bulkValue}
                onChange={(event) => setBulkValue(event.target.value)}
              >
                <option value="">{s.t("editor.bulk.choose")}</option>
                {bulkChoiceValues.map((choice) => (
                  <option key={choice} value={choice}>
                    {choiceLabel(bulkField, choice, bulkColumn)}
                  </option>
                ))}
              </select>
            ) : bulkColumn?.kind === "compactDate" ? (
              <CompactDateInput
                key={bulkField}
                value={bulkValue}
                label={s.t(bulkColumn.labelKey)}
                s={s}
                onChange={setBulkValue}
              />
            ) : (
              <>
                <input
                  aria-label={s.t("editor.bulk.value")}
                  type={bulkColumn?.kind === "int" ? "number" : "text"}
                  value={bulkValue}
                  list={bulkSuggestions.length ? bulkListId : undefined}
                  onChange={(event) => setBulkValue(event.target.value)}
                />
                {bulkSuggestions.length > 0 && (
                  <datalist id={bulkListId}>
                    {bulkSuggestions.map((suggestion) => (
                      <option key={suggestion} value={suggestion} />
                    ))}
                  </datalist>
                )}
              </>
            )}
            <Button
              variant="secondary"
              disabled={!selectedIds.size || !bulkField || bulkValue === ""}
              onClick={applyBulk}
            >
              {s.t("editor.bulk.apply")}
            </Button>
          </div>
        </div>
      )}
      <div className="editor-scroll">
        <table className="editor-table">
          <thead>
            <tr>
              {editor.selectionEnabled === true && (
                <th className="selection-column">
                  <input
                    aria-label={s.t("editor.selectAll")}
                    type="checkbox"
                    checked={
                      filteredRowIndices.length > 0 &&
                      filteredRowIndices.every((index) =>
                        selectedIds.has(rowId(rows[index], index)),
                      )
                    }
                    onChange={(event) =>
                      event.target.checked ? selectAll() : selectNone()
                    }
                  />
                </th>
              )}
              {visibleColumns.map((column) => {
                const defaultWidth =
                  column.kind === "bool"
                    ? 50
                    : column.key === "sourceStatuses" ||
                        column.key === "disposition"
                      ? 220
                      : column.key === "reasonKey"
                        ? 180
                        : column.key === "currentStatus" ||
                            column.key === "proposedStatus"
                          ? 130
                          : column.key === "rfiNo"
                            ? 120
                            : column.key === "_sheet"
                              ? 60
                              : column.key === "_row"
                                ? 50
                                : 120;
                const colWidth = column.width ?? defaultWidth;
                const isFixedNarrow =
                  column.kind === "bool" ||
                  column.key === "_sheet" ||
                  column.key === "_row" ||
                  colWidth <= 65;
                return (
                  <th
                    key={column.key}
                    style={
                      editor.layout === "compact"
                        ? isFixedNarrow
                          ? {
                              width: colWidth,
                              minWidth: colWidth,
                              maxWidth: colWidth,
                            }
                          : {
                              minWidth: colWidth,
                            }
                        : { minWidth: colWidth }
                    }
                  >
                    {s.t(column.labelKey)}
                  </th>
                );
              })}
              {(editor.reorderable === true ||
                editor.duplicateEnabled === true) && (
                <th
                  className="editor-actions-header"
                  aria-label={s.t("editor.rowActions")}
                />
              )}
            </tr>
          </thead>
          <tbody>
            {filteredRowIndices.map((rowIndex) => {
              const row = rows[rowIndex];
              const sheet = String(row._sheet ?? "");
              const groupCount = rows.filter(
                (item) => item[groupField] === row[groupField],
              ).length;
              const id = rowId(row, rowIndex);
              return (
                <tr
                  key={id}
                  className={selectedIds.has(id) ? "selected-row" : ""}
                  tabIndex={0}
                  onKeyDown={(event) => {
                    if (
                      event.key === " " &&
                      event.target === event.currentTarget
                    ) {
                      event.preventDefault();
                      if (editor.selectionEnabled === true) {
                        toggleRow(id);
                      } else if (
                        typeof row.enabled === "boolean" &&
                        row.selectable !== false
                      ) {
                        update(rowIndex, "enabled", !row.enabled);
                      }
                    }
                  }}
                >
                  {editor.selectionEnabled === true && (
                    <td className="selection-column">
                      <input
                        aria-label={s.t("editor.selectRow", {
                          row: rowIndex + 1,
                        })}
                        type="checkbox"
                        checked={selectedIds.has(id)}
                        onChange={() => toggleRow(id)}
                      />
                      {groupCount > 1 && (
                        <button
                          type="button"
                          title={s.t("editor.selectGroup")}
                          onClick={() =>
                            selectGroup(groupField, row[groupField])
                          }
                        >
                          {groupCount}
                        </button>
                      )}
                    </td>
                  )}
                  {visibleColumns.map((column) => {
                    if (column.sheets && !column.sheets.includes(sheet)) {
                      return (
                        <td key={column.key} className="not-applicable">
                          —
                        </td>
                      );
                    }
                    const text = String(row[column.key] ?? "");
                    const displayText =
                      column.display === "localizedKey"
                        ? s.t(text)
                        : column.key === "status" && text
                          ? s.t(`print.status.${text}`)
                          : column.key === "worksheetVisibility" && text
                            ? s.t(`print.worksheet.visibility.${text}`)
                            : displayEditorValue(column, text);
                    const choices = choicesFor(column, row);
                    const suggestions = Array.isArray(autocomplete[column.key])
                      ? (autocomplete[column.key] as unknown[]).map(String)
                      : [];
                    const listId = `editor-${rowIndex}-${column.key}`;
                    return (
                      <td
                        key={column.key}
                        className={
                          column.kind === "bool" ? "center" : undefined
                        }
                      >
                        {column.kind === "readonly" ? (
                          <span
                            className="readonly-cell"
                            title={displayText || text}
                          >
                            {displayText}
                          </span>
                        ) : column.kind === "bool" ? (
                          <input
                            aria-label={s.t(column.labelKey)}
                            type="checkbox"
                            checked={Boolean(row[column.key])}
                            disabled={
                              column.disabledWhen !== undefined &&
                              row[column.disabledWhen.field] ===
                                column.disabledWhen.equals
                            }
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.checked)
                            }
                          />
                        ) : column.kind === "choice" ? (
                          <select
                            aria-label={s.t(column.labelKey)}
                            title={text}
                            value={text}
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.value)
                            }
                          >
                            {choices.map((choice) => (
                              <option key={choice} value={choice}>
                                {choiceLabel(column.key, choice, column)}
                              </option>
                            ))}
                          </select>
                        ) : column.kind === "compactDate" ? (
                          <CompactDateInput
                            value={text}
                            label={s.t(column.labelKey)}
                            s={s}
                            onChange={(value) =>
                              update(rowIndex, column.key, value)
                            }
                          />
                        ) : column.kind === "multiline" ? (
                          <textarea
                            rows={2}
                            value={text}
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.value)
                            }
                          />
                        ) : (
                          <>
                            <input
                              type={column.kind === "int" ? "number" : "text"}
                              value={text}
                              list={suggestions.length ? listId : undefined}
                              onChange={(event) =>
                                update(rowIndex, column.key, event.target.value)
                              }
                            />
                            {suggestions.length > 0 && (
                              <datalist id={listId}>
                                {suggestions.map((suggestion) => (
                                  <option key={suggestion} value={suggestion} />
                                ))}
                              </datalist>
                            )}
                          </>
                        )}
                      </td>
                    );
                  })}
                  {(editor.reorderable === true ||
                    editor.duplicateEnabled === true) && (
                    <td className="editor-row-actions">
                      {editor.reorderable === true && (
                        <>
                          <button
                            type="button"
                            aria-label={s.t("action.moveUp")}
                            title={s.t("action.moveUp")}
                            disabled={rowIndex === 0}
                            onClick={() => move(rowIndex, -1)}
                          >
                            <ArrowUp />
                          </button>
                          <button
                            type="button"
                            aria-label={s.t("action.moveDown")}
                            title={s.t("action.moveDown")}
                            disabled={rowIndex === rows.length - 1}
                            onClick={() => move(rowIndex, 1)}
                          >
                            <ArrowDown />
                          </button>
                        </>
                      )}
                      {editor.duplicateEnabled === true && (
                        <>
                          <button
                            type="button"
                            title={s.t("rfi.action.duplicate")}
                            onClick={() => duplicate(rowIndex)}
                          >
                            <Copy />
                          </button>
                          <button
                            type="button"
                            title={s.t("rfi.action.removeDuplicate")}
                            disabled={groupCount <= 1}
                            onClick={() => remove(rowIndex)}
                          >
                            <Trash2 />
                          </button>
                        </>
                      )}
                    </td>
                  )}
                </tr>
              );
            })}
            {filteredRowIndices.length === 0 && (
              <tr>
                <td
                  colSpan={visibleColumns.length + 2}
                  className="empty-filter-cell"
                >
                  {s.t("plan.empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

export function isPlanEditorReady(
  editor: Record<string, unknown>,
  rows: Record<string, unknown>[],
): boolean {
  const requiredFields = Array.isArray(editor.requiredFields)
    ? editor.requiredFields.filter(
        (field): field is string => typeof field === "string",
      )
    : [];
  return rows.every(
    (row) =>
      row.enabled === false ||
      requiredFields.every((field) => {
        const value = row[field];
        return typeof value === "string"
          ? value.trim().length > 0
          : value !== null && value !== undefined;
      }),
  );
}

export function PlanView({
  plan,
  s,
  error,
  onCancel,
  onApply,
  editorRows,
  onEditorRowsChange,
}: {
  plan: Plan;
  s: Strings;
  error: string;
  onCancel: () => void;
  onApply: () => void;
  editorRows?: Record<string, unknown>[];
  onEditorRowsChange?: (rows: Record<string, unknown>[]) => void;
}) {
  const [warningFocusCode, setWarningFocusCode] = useState("");
  const moves = plan.steps.filter((step) => step.kind !== "mkdir").length;
  const editor =
    plan.meta?.editor && typeof plan.meta.editor === "object"
      ? (plan.meta.editor as Record<string, unknown>)
      : null;
  const editorReady =
    !editor || !editorRows || isPlanEditorReady(editor, editorRows);
  const editorCanMaterializeStep = Boolean(
    editorRows?.some((row) => row.enabled !== false) &&
    ((Array.isArray(editor?.requiredFields) &&
      editor.requiredFields.length > 0) ||
      (plan.opId === "files.batch_print" && editor?.kind === "recordTable")),
  );
  const canApply =
    editorReady && (plan.steps.length > 0 || editorCanMaterializeStep);
  const replyPreview = outlookReplyPreview(plan.meta?.replyPreview);
  const focusableWarningCodes =
    plan.opId === "rfi.stage_signed_first_pages"
      ? (editorRows ?? [])
          .map((row) => (typeof row.code === "string" ? row.code : ""))
          .filter(Boolean)
      : [];
  const signedStage = plan.opId === "rfi.stage_signed_first_pages";
  return (
    <div className="flow-layout">
      <div className="flow-scroll plan-flow-scroll">
        <PlanImpactSummary plan={plan} s={s} />
        <ScanIndexSummary plan={plan} s={s} />
        {editor && editorRows && onEditorRowsChange ? (
          <>
            {signedStage && (
              <SignedStageNavigator plan={plan} rows={editorRows} s={s} />
            )}
            {!signedStage && plan.warnings.length > 0 && (
              <PlanWarnings warnings={plan.warnings} focusableCodes={[]} s={s} />
            )}
            <WorkbookChangePreview plan={plan} s={s} />
            {replyPreview && (
              <section
                className="outlook-reply-preview"
                aria-labelledby="outlook-reply-preview-title"
              >
                <h3 id="outlook-reply-preview-title">
                  {s.t("outlookDraft.previewTitle")}
                </h3>
                <div>
                  <strong>{s.t("outlookDraft.subject")}</strong>
                  <pre>{replyPreview.subject}</pre>
                </div>
                <div>
                  <strong>{s.t("outlookDraft.plainText")}</strong>
                  <pre>{replyPreview.plainText}</pre>
                </div>
                <div>
                  <strong>{s.t("outlookDraft.html")}</strong>
                  <pre>{replyPreview.html}</pre>
                </div>
              </section>
            )}
            {signedStage ? (
              <>
                <div id="signed-approved-records">
                  <EditablePlanTable
                    editor={editor}
                    rows={editorRows}
                    s={s}
                    onChange={onEditorRowsChange}
                  />
                </div>
                <SignedPageComparison
                  plan={plan}
                  editorRows={editorRows}
                  focusCode={warningFocusCode}
                  s={s}
                />
                <SignedWarningReviewSummary plan={plan} s={s} />
                {plan.warnings.length > 0 && (
                  <PlanWarnings
                    warnings={plan.warnings}
                    focusableCodes={focusableWarningCodes}
                    onFocusCode={setWarningFocusCode}
                    s={s}
                  />
                )}
              </>
            ) : (
              <EditablePlanTable
                editor={editor}
                rows={editorRows}
                s={s}
                onChange={onEditorRowsChange}
              />
            )}
            {plan.steps.length > 0 && <PlanExactEffects plan={plan} s={s} />}
          </>
        ) : (
          <>
            {plan.warnings.length > 0 && (
              <PlanWarnings
                warnings={plan.warnings}
                focusableCodes={[]}
                s={s}
              />
            )}
            <WorkbookChangePreview plan={plan} s={s} />
            {replyPreview && (
              <section
                className="outlook-reply-preview"
                aria-labelledby="outlook-reply-preview-title"
              >
                <h3 id="outlook-reply-preview-title">
                  {s.t("outlookDraft.previewTitle")}
                </h3>
                <div>
                  <strong>{s.t("outlookDraft.subject")}</strong>
                  <pre>{replyPreview.subject}</pre>
                </div>
                <div>
                  <strong>{s.t("outlookDraft.plainText")}</strong>
                  <pre>{replyPreview.plainText}</pre>
                </div>
                <div>
                  <strong>{s.t("outlookDraft.html")}</strong>
                  <pre>{replyPreview.html}</pre>
                </div>
              </section>
            )}
            <section className="plan-section">
              <div className="plan-heading">
                <strong>{s.t("plan.summary", { count: moves })}</strong>
                <span>
                  <Eye /> {s.t("plan.readOnly")}
                </span>
              </div>
              {plan.steps.length ? (
                <div className="step-list">
                  {plan.steps.map((step, index) => (
                    <StepRow key={`${step.kind}-${index}`} step={step} s={s} />
                  ))}
                </div>
              ) : (
                <EmptyState icon={<FileCheck2 />} title={s.t("plan.empty")} />
              )}
            </section>
          </>
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </div>
      <ActionBar
        left={
          <Button variant="ghost" onClick={onCancel}>
            {s.t("action.cancel")}
          </Button>
        }
        right={
          <Button icon={<Play />} disabled={!canApply} onClick={onApply}>
            {s.t("action.apply")}
          </Button>
        }
      />
    </div>
  );
}

export interface PlanImpact {
  outputs: number;
  updates: number;
  moves: number;
  printJobs: number;
  drafts: number;
  cleanup: number;
  warnings: number;
}

export function summarizePlanImpact(plan: Plan): PlanImpact {
  const impact: PlanImpact = {
    outputs: 0,
    updates: 0,
    moves: 0,
    printJobs: 0,
    drafts: 0,
    cleanup: 0,
    warnings: plan.warnings.length,
  };
  for (const step of plan.steps) {
    switch (step.kind) {
      case "copy":
      case "write":
      case "pdf_first_page_replace":
      case "pdf_page_rotate":
      case "pdf_pages_extract":
        impact.outputs += 1;
        break;
      case "workbook_update":
      case "restore_backup":
      case "replica_publish":
        impact.updates += 1;
        break;
      case "move":
      case "rename":
        impact.moves += 1;
        break;
      case "print":
        impact.printJobs += 1;
        break;
      case "outlook_reply_draft":
        impact.drafts += 1;
        break;
      case "cleanup_temp":
        impact.cleanup += 1;
        break;
      case "mkdir":
        break;
    }
  }
  return impact;
}

function PlanImpactSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const impact = summarizePlanImpact(plan);
  const entries = (
    [
      ["plan.impact.outputs", impact.outputs],
      ["plan.impact.updates", impact.updates],
      ["plan.impact.moves", impact.moves],
      ["plan.impact.printJobs", impact.printJobs],
      ["plan.impact.drafts", impact.drafts],
      ["plan.impact.cleanup", impact.cleanup],
      ["plan.impact.warnings", impact.warnings],
    ] satisfies [string, number][]
  ).filter(([, count]) => count > 0);
  return (
    <section className="plan-impact" aria-labelledby="plan-impact-title">
      <div className="plan-impact-heading">
        <Shield aria-hidden="true" />
        <div>
          <strong id="plan-impact-title">{s.t("plan.impact.title")}</strong>
          <span>{s.t("plan.impact.previewOnly")}</span>
        </div>
      </div>
      {entries.length > 0 && (
        <dl className="plan-impact-counts">
          {entries.map(([key, count]) => (
            <div key={key}>
              <dt>{s.t(key)}</dt>
              <dd>{count}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

function ScanIndexSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const raw = plan.meta?.scanIndex;
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const data = raw as Record<string, unknown>;
  const count = (key: string) =>
    typeof data[key] === "number" && Number.isFinite(data[key])
      ? Math.max(0, Math.trunc(data[key] as number))
      : 0;
  const entries: [string, number][] = [
    ["scanIndex.discovered", count("discovered")],
    ["scanIndex.reused", count("reused")],
    ["scanIndex.analyzed", count("analyzed")],
    ["scanIndex.failed", count("failed")],
    ["scanIndex.interrupted", count("priorInterruptedRuns")],
  ];
  const visibleEntries = entries.filter(
    ([key, value]) => key !== "scanIndex.interrupted" || value > 0,
  );
  return (
    <section
      className="plan-impact scan-index-summary"
      aria-labelledby="scan-index-title"
    >
      <div className="plan-impact-heading">
        <Database aria-hidden="true" />
        <div>
          <strong id="scan-index-title">{s.t("scanIndex.title")}</strong>
          <span>
            {data.forceRescan === true
              ? s.t("scanIndex.forceApplied")
              : s.t("scanIndex.help")}
          </span>
        </div>
      </div>
      <dl className="plan-impact-counts">
        {visibleEntries.map(([key, value]) => (
          <div key={key}>
            <dt>{s.t(key)}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function PlanExactEffects({ plan, s }: { plan: Plan; s: Strings }) {
  const effects = plan.steps.filter((step) => step.kind !== "mkdir");
  if (effects.length === 0) return null;
  return (
    <details className="plan-exact-effects">
      <summary>
        <Eye aria-hidden="true" />
        <span>{s.t("plan.exact.title", { count: effects.length })}</span>
        <small>{s.t("plan.exact.help")}</small>
      </summary>
      <div className="plan-exact-list">
        {effects.map((step, index) => (
          <StepRow key={`${step.kind}-${index}`} step={step} s={s} fullPath />
        ))}
      </div>
    </details>
  );
}

export function canonicalRfiInText(value: string): string | null {
  return (
    value.match(
      /(?:^|[^A-Z0-9-])(AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4})(?=$|[^A-Z0-9-])/,
    )?.[1] ?? null
  );
}

type WorkbookCellChange = {
  workbook: string;
  sheet: string;
  cell: string;
  before: string;
  after: string;
  rfiCode: string;
};

const MAX_WORKBOOK_CELL_PREVIEWS = 1_000;

function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function previewText(value: unknown): string | null {
  return typeof value === "string" && value.length <= 500 ? value : null;
}

function excelCell(row: unknown, column: unknown): string | null {
  if (
    typeof row !== "number" ||
    typeof column !== "number" ||
    !Number.isInteger(row) ||
    !Number.isInteger(column) ||
    row < 1 ||
    row > 1_048_576 ||
    column < 1 ||
    column > 16_384
  ) {
    return null;
  }
  let value = column;
  let letters = "";
  while (value > 0) {
    value -= 1;
    letters = String.fromCharCode(65 + (value % 26)) + letters;
    value = Math.floor(value / 26);
  }
  return `${letters}${row}`;
}

export function summarizeWorkbookCellChanges(plan: Plan): WorkbookCellChange[] {
  const result: WorkbookCellChange[] = [];
  const rowCodes = new Map<string, string>();
  const editor = recordValue(plan.meta?.editor);
  const editorRows = Array.isArray(editor?.rows) ? editor.rows : [];
  for (const rawRow of editorRows.slice(0, 2_000)) {
    const row = recordValue(rawRow);
    const rowId = previewText(row?._rowId);
    const code = previewText(row?.canonicalRfi) ?? previewText(row?.rfiNo);
    if (rowId && code && canonicalRfiInText(code) === code)
      rowCodes.set(rowId, code);
  }
  const add = (change: WorkbookCellChange) => {
    if (result.length < MAX_WORKBOOK_CELL_PREVIEWS) result.push(change);
  };
  for (const step of plan.steps) {
    if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    if (step.kind !== "workbook_update") continue;
    const sync = recordValue(step.meta?.syncStep);
    const syncUpdates = recordValue(sync?.updates);
    if (sync && syncUpdates) {
      const workbook =
        sync.workbookKind === "rfiLog" || sync.workbookKind === "followUp"
          ? sync.workbookKind
          : "workbook";
      for (const [sheetName, rawSheet] of Object.entries(syncUpdates)) {
        const sheet = previewText(sheetName);
        const sheetUpdates = recordValue(rawSheet);
        if (!sheet || !sheetUpdates) continue;
        for (const rawUpdate of Object.values(sheetUpdates)) {
          const update = recordValue(rawUpdate);
          const cell = excelCell(update?.row, update?.col);
          const before = previewText(update?.expectedValue);
          const after = previewText(update?.replacementValue);
          const rowId = previewText(update?.rowId);
          if (!cell || before === null || after === null || !rowId) continue;
          add({
            workbook,
            sheet,
            cell,
            before,
            after,
            rfiCode: rowCodes.get(rowId) ?? "",
          });
          if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
        }
      }
      continue;
    }
    const updates = Array.isArray(step.meta?.updates) ? step.meta.updates : [];
    for (const rawUpdate of updates) {
      const update = recordValue(rawUpdate);
      const target = recordValue(update?.target);
      const worksheet = recordValue(target?.worksheet);
      const statusCell = recordValue(target?.statusCell);
      const rfiCell = recordValue(target?.rfiCell);
      const sheet = previewText(worksheet?.name);
      const cell = excelCell(target?.row, target?.statusColumn);
      const before = previewText(statusCell?.value);
      const after = previewText(update?.proposedStatus);
      const candidateCode = previewText(update?.canonicalRfi) ?? "";
      const rfiCode =
        canonicalRfiInText(candidateCode) === candidateCode
          ? candidateCode
          : "";
      const rfiAddress = excelCell(target?.row, target?.rfiColumn);
      const rfiBefore = previewText(rfiCell?.value);
      const rfiAfter = previewText(update?.proposedRfi);
      if (
        sheet &&
        rfiAddress &&
        rfiBefore !== null &&
        rfiAfter !== null &&
        rfiBefore !== rfiAfter
      ) {
        add({
          workbook: "followUp",
          sheet,
          cell: rfiAddress,
          before: rfiBefore,
          after: rfiAfter,
          rfiCode,
        });
        if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
      }
      if (!sheet || !cell || before === null || after === null) continue;
      add({ workbook: "followUp", sheet, cell, before, after, rfiCode });
      if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    }
  }
  return result;
}

function WorkbookChangePreview({ plan, s }: { plan: Plan; s: Strings }) {
  const changes = summarizeWorkbookCellChanges(plan);
  if (changes.length === 0) return null;
  return (
    <details className="workbook-change-preview">
      <summary>
        <span>
          <TableProperties aria-hidden="true" />
          <strong>{s.t("plan.workbookChanges.title")}</strong>
        </span>
        <small>
          {s.t("plan.workbookChanges.count", { count: changes.length })}
        </small>
      </summary>
      <p>{s.t("plan.workbookChanges.help")}</p>
      <div className="workbook-change-table-wrap">
        <table>
          <thead>
            <tr>
              <th>{s.t("plan.workbookChanges.workbook")}</th>
              <th>{s.t("plan.workbookChanges.sheetCell")}</th>
              <th>{s.t("plan.workbookChanges.rfi")}</th>
              <th>{s.t("plan.workbookChanges.before")}</th>
              <th>{s.t("plan.workbookChanges.after")}</th>
            </tr>
          </thead>
          <tbody>
            {changes.map((change, index) => (
              <tr
                key={`${change.workbook}-${change.sheet}-${change.cell}-${index}`}
              >
                <td>{s.t(`plan.workbookChanges.kind.${change.workbook}`)}</td>
                <td>
                  <code>
                    {change.sheet}!{change.cell}
                  </code>
                </td>
                <td>{change.rfiCode || "—"}</td>
                <td>{change.before || s.t("plan.workbookChanges.blank")}</td>
                <td>{change.after || s.t("plan.workbookChanges.blank")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {changes.length === MAX_WORKBOOK_CELL_PREVIEWS && (
        <small className="workbook-change-limit">
          {s.t("plan.workbookChanges.limit", {
            count: MAX_WORKBOOK_CELL_PREVIEWS,
          })}
        </small>
      )}
    </details>
  );
}

export function PlanWarnings({
  warnings,
  focusableCodes,
  onFocusCode,
  s,
}: {
  warnings: Plan["warnings"];
  focusableCodes: string[];
  onFocusCode?: (code: string) => void;
  s: Strings;
}) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const types = [...new Set(warnings.map((warning) => warning.key))];
  const normalizedQuery = query.trim().toLocaleLowerCase(s.locale);
  const visible = warnings.filter((warning) => {
    if (type !== "all" && warning.key !== type) return false;
    if (!normalizedQuery) return true;
    return `${warning.subject} ${s.t(warning.key)}`
      .toLocaleLowerCase(s.locale)
      .includes(normalizedQuery);
  });
  const focusable = new Set(focusableCodes);
  return (
    <section
      id="signed-warning-list"
      className="warning-review"
      aria-labelledby="warning-review-title"
    >
      <div className="warning-review-heading">
        <div>
          <CircleAlert aria-hidden="true" />
          <strong id="warning-review-title">{s.t("ui.warnings")}</strong>
          <span>{warnings.length}</span>
        </div>
        {warnings.length >= 5 && (
          <div className="warning-review-filters">
            <input
              type="search"
              aria-label={s.t("plan.warnings.search")}
              placeholder={s.t("plan.warnings.search")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <select
              aria-label={s.t("plan.warnings.type")}
              value={type}
              onChange={(event) => setType(event.target.value)}
            >
              <option value="all">{s.t("plan.warnings.all")}</option>
              {types.map((key) => (
                <option key={key} value={key}>
                  {s.t(key)}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>
      <div className="warning-review-list" role="list">
        {visible.map((warning, index) => {
          const code = canonicalRfiInText(warning.subject);
          const canFocus = Boolean(code && focusable.has(code) && onFocusCode);
          return (
            <div
              key={`${warning.key}-${warning.subject}-${index}`}
              role="listitem"
            >
              <div>
                {warning.subject && <strong>{warning.subject}</strong>}
                <span>{s.t(warning.key)}</span>
              </div>
              {canFocus && code && (
                <button type="button" onClick={() => onFocusCode?.(code)}>
                  <Eye aria-hidden="true" />
                  {s.t("plan.warnings.focus")}
                </button>
              )}
            </div>
          );
        })}
        {visible.length === 0 && <p>{s.t("plan.warnings.none")}</p>}
      </div>
      {warnings.length >= 5 && (
        <small>
          {s.t("plan.warnings.showing", {
            shown: visible.length,
            count: warnings.length,
          })}
        </small>
      )}
    </section>
  );
}

interface SignedWarningReason {
  key: string;
  pageNumber: number | null;
  code: string;
  detail: string;
}

interface SignedWarningReviewItem {
  sourceFile: string;
  destination: string;
  status: "planned" | "exists" | "blocked";
  reasons: SignedWarningReason[];
}

function signedWarningReviewItems(plan: Plan): SignedWarningReviewItem[] {
  const raw = Array.isArray(plan.meta?.warningReviewItems)
    ? plan.meta.warningReviewItems
    : [];
  return raw.flatMap((value) => {
    if (!value || typeof value !== "object") return [];
    const item = value as Record<string, unknown>;
    if (
      typeof item.sourceFile !== "string" ||
      typeof item.destination !== "string" ||
      !["planned", "exists", "blocked"].includes(String(item.status)) ||
      !Array.isArray(item.reasons)
    ) {
      return [];
    }
    const reasons = item.reasons.flatMap((rawReason) => {
      if (!rawReason || typeof rawReason !== "object") return [];
      const reason = rawReason as Record<string, unknown>;
      if (typeof reason.key !== "string" || typeof reason.detail !== "string") {
        return [];
      }
      return [
        {
          key: reason.key,
          pageNumber:
            typeof reason.pageNumber === "number" &&
            Number.isSafeInteger(reason.pageNumber) &&
            reason.pageNumber > 0
              ? reason.pageNumber
              : null,
          code: typeof reason.code === "string" ? reason.code : "",
          detail: reason.detail,
        },
      ];
    });
    return [
      {
        sourceFile: item.sourceFile,
        destination: item.destination,
        status: item.status as SignedWarningReviewItem["status"],
        reasons,
      },
    ];
  });
}

function scrollToSignedSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ block: "start" });
}

function SignedStageNavigator({
  plan,
  rows,
  s,
}: {
  plan: Plan;
  rows: Record<string, unknown>[];
  s: Strings;
}) {
  const reviewItems = signedWarningReviewItems(plan);
  const approved = rows.filter((row) => row.enabled !== false).length;
  return (
    <nav className="signed-stage-nav" aria-label={s.t("rfi.signed.nav.label")}>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-approved-records")}
      >
        <BadgeCheck aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.approved")}</span>
        <strong>{approved}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-page-comparison")}
      >
        <Files aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.compare")}</span>
        <strong>{rows.length}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-warning-review")}
        disabled={reviewItems.length === 0}
      >
        <CircleAlert aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.review")}</span>
        <strong>{reviewItems.length}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-warning-list")}
        disabled={plan.warnings.length === 0}
      >
        <FileSearch aria-hidden="true" />
        <span>{s.t("ui.warnings")}</span>
        <strong>{plan.warnings.length}</strong>
      </button>
    </nav>
  );
}

function SignedWarningReviewSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const items = signedWarningReviewItems(plan);
  if (items.length === 0) return null;
  return (
    <section
      id="signed-warning-review"
      className="signed-warning-review"
      aria-labelledby="signed-warning-review-title"
    >
      <header>
        <div>
          <CircleAlert aria-hidden="true" />
          <span>
            <strong id="signed-warning-review-title">
              {s.t("rfi.signed.warningReview.title")}
            </strong>
            <small>{s.t("rfi.signed.warningReview.help")}</small>
          </span>
        </div>
        <b>{items.length}</b>
      </header>
      <div className="signed-warning-review-list">
        {items.map((item) => (
          <article key={`${item.sourceFile}-${item.destination}`}>
            <div className="signed-warning-review-file">
              <strong>{item.sourceFile}</strong>
              <span className={`review-copy-status ${item.status}`}>
                {s.t(`rfi.signed.warningReview.${item.status}`)}
              </span>
            </div>
            <small>
              {s.t("rfi.signed.warningReview.destination")}: {item.destination}
            </small>
            <ul>
              {item.reasons.map((reason, index) => (
                <li key={`${reason.key}-${reason.pageNumber}-${index}`}>
                  <span>
                    {reason.pageNumber
                      ? s.t("rfi.signed.warningReview.page", {
                          page: reason.pageNumber,
                        })
                      : s.t("rfi.signed.warningReview.file")}
                    {reason.code ? ` · ${reason.code}` : ""}
                  </span>
                  <strong>{s.t(reason.key)}</strong>
                  {reason.detail && <small>{reason.detail}</small>}
                </li>
              ))}
            </ul>
          </article>
        ))}
      </div>
    </section>
  );
}

export function safePngThumbnail(value: unknown): string | null {
  const prefix = "data:image/png;base64,";
  const pngSignature = "iVBORw0KGgo";
  const payload = typeof value === "string" ? value.slice(prefix.length) : "";
  if (
    typeof value !== "string" ||
    !value.startsWith(prefix) ||
    value.length > 1_100_000 ||
    !payload.startsWith(pngSignature) ||
    payload.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(payload)
  ) {
    return null;
  }
  return value;
}

function SignedPageComparison({
  plan,
  editorRows,
  focusCode,
  s,
}: {
  plan: Plan;
  editorRows: Record<string, unknown>[];
  focusCode: string;
  s: Strings;
}) {
  const sectionRef = useRef<HTMLElement>(null);
  const raw =
    plan.meta?.pageComparisons && typeof plan.meta.pageComparisons === "object"
      ? (plan.meta.pageComparisons as Record<string, unknown>)
      : {};
  const available = editorRows.filter((row) => {
    const rowId = typeof row._rowId === "string" ? row._rowId : "";
    const comparison = raw[rowId];
    if (!comparison || typeof comparison !== "object") return false;
    const record = comparison as Record<string, unknown>;
    const archives =
      record.archives && typeof record.archives === "object"
        ? Object.values(record.archives as Record<string, unknown>)
        : [];
    return Boolean(
      safePngThumbnail(record.signed) || archives.some(safePngThumbnail),
    );
  });
  const availableIds = available.map((row) => String(row._rowId));
  const [selectedId, setSelectedId] = useState(availableIds[0] ?? "");
  const [expanded, setExpanded] = useState<"archive" | "signed" | null>(null);
  const availableSignature = availableIds.join("\u0000");
  useEffect(() => {
    if (!availableIds.includes(selectedId)) {
      setSelectedId(availableIds[0] ?? "");
    }
  }, [availableSignature, selectedId]);
  useEffect(() => {
    if (!focusCode) return;
    const row = available.find((candidate) => candidate.code === focusCode);
    if (!row) return;
    setSelectedId(String(row._rowId));
    sectionRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [availableSignature, focusCode]);
  const omitted =
    typeof plan.meta?.pagePreviewOmitted === "number"
      ? plan.meta.pagePreviewOmitted
      : 0;
  if (available.length === 0) {
    return omitted > 0 ? (
      <Notice kind="warning">{s.t("rfi.signed.previewUnavailable")}</Notice>
    ) : null;
  }
  const selected =
    available.find((row) => row._rowId === selectedId) ?? available[0];
  const comparison = raw[String(selected._rowId)] as Record<string, unknown>;
  const archives =
    comparison.archives && typeof comparison.archives === "object"
      ? (comparison.archives as Record<string, unknown>)
      : {};
  const archiveToken =
    typeof selected.archiveFile === "string" ? selected.archiveFile : "";
  const archivePreview = safePngThumbnail(archives[archiveToken]);
  const signedPreview = safePngThumbnail(comparison.signed);
  const code = typeof selected.code === "string" ? selected.code : selectedId;
  const signedFile =
    typeof selected.signedFile === "string" ? selected.signedFile : "";
  const choiceLabels =
    plan.meta?.editor && typeof plan.meta.editor === "object"
      ? ((plan.meta.editor as Record<string, unknown>).choiceLabels as
          | Record<string, unknown>
          | undefined)
      : undefined;
  const archiveLabels =
    choiceLabels?.archiveFile && typeof choiceLabels.archiveFile === "object"
      ? (choiceLabels.archiveFile as Record<string, unknown>)
      : {};
  const archiveFile =
    typeof archiveLabels[archiveToken] === "string"
      ? String(archiveLabels[archiveToken])
      : "";
  const selectedIndex = available.findIndex(
    (row) => String(row._rowId) === String(selected._rowId),
  );
  const selectOffset = (offset: number) => {
    const target = available[selectedIndex + offset];
    if (target) setSelectedId(String(target._rowId));
  };
  return (
    <>
      <section
        id="signed-page-comparison"
        ref={sectionRef}
        className="signed-page-comparison"
        aria-labelledby="signed-page-comparison-title"
      >
        <div className="signed-page-comparison-heading">
          <div>
            <strong id="signed-page-comparison-title">
              {s.t("rfi.signed.previewTitle")}
            </strong>
            <span>{s.t("rfi.signed.previewHelp")}</span>
          </div>
          <div className="signed-page-comparison-selector">
            <button
              type="button"
              aria-label={s.t("rfi.signed.previewPrevious")}
              disabled={selectedIndex <= 0}
              onClick={() => selectOffset(-1)}
            >
              <ArrowLeft aria-hidden="true" />
            </button>
            {available.length > 1 ? (
              <select
                aria-label={s.t("rfi.signed.previewRfi")}
                value={String(selected._rowId)}
                onChange={(event) => setSelectedId(event.target.value)}
              >
                {available.map((row) => (
                  <option key={String(row._rowId)} value={String(row._rowId)}>
                    {typeof row.code === "string"
                      ? row.code
                      : String(row._rowId)}
                  </option>
                ))}
              </select>
            ) : (
              <span className="signed-page-comparison-code">{code}</span>
            )}
            <button
              type="button"
              aria-label={s.t("rfi.signed.previewNext")}
              disabled={selectedIndex >= available.length - 1}
              onClick={() => selectOffset(1)}
            >
              <ArrowRight aria-hidden="true" />
            </button>
          </div>
        </div>
        <div className="signed-page-change-summary">
          <span>{s.t("rfi.signed.previewArchive")}</span>
          <ArrowRight aria-hidden="true" />
          <strong>{s.t("rfi.signed.previewSigned")}</strong>
          <small>{s.t("rfi.signed.previewOnlyPageOne")}</small>
        </div>
        <div className="signed-page-comparison-grid">
          <figure>
            <figcaption>
              <span>{s.t("rfi.signed.previewBefore")}</span>
              <strong>{s.t("rfi.signed.previewArchive")}</strong>
              {archiveFile && <small title={archiveFile}>{archiveFile}</small>}
            </figcaption>
            {archivePreview ? (
              <button
                type="button"
                className="signed-page-preview-button"
                aria-label={`${s.t("rfi.signed.previewArchiveAlt", { code })} · ${s.t("rfi.signed.previewEnlarge")}`}
                onClick={() => setExpanded("archive")}
              >
                <img
                  src={archivePreview}
                  alt={s.t("rfi.signed.previewArchiveAlt", { code })}
                />
                <span>{s.t("rfi.signed.previewEnlarge")}</span>
              </button>
            ) : (
              <div className="signed-page-preview-empty">
                {archiveToken
                  ? s.t("rfi.signed.previewUnavailable")
                  : s.t("rfi.signed.previewChooseArchive")}
              </div>
            )}
          </figure>
          <figure>
            <figcaption>
              <span>{s.t("rfi.signed.previewAfter")}</span>
              <strong>{s.t("rfi.signed.previewSigned")}</strong>
              {signedFile && <small title={signedFile}>{signedFile}</small>}
            </figcaption>
            {signedPreview ? (
              <button
                type="button"
                className="signed-page-preview-button"
                aria-label={`${s.t("rfi.signed.previewSignedAlt", { code })} · ${s.t("rfi.signed.previewEnlarge")}`}
                onClick={() => setExpanded("signed")}
              >
                <img
                  src={signedPreview}
                  alt={s.t("rfi.signed.previewSignedAlt", { code })}
                />
                <span>{s.t("rfi.signed.previewEnlarge")}</span>
              </button>
            ) : (
              <div className="signed-page-preview-empty">
                {s.t("rfi.signed.previewUnavailable")}
              </div>
            )}
          </figure>
        </div>
        {omitted > 0 && (
          <small>{s.t("rfi.signed.previewOmitted", { count: omitted })}</small>
        )}
      </section>
      {expanded && (expanded === "archive" ? archivePreview : signedPreview) && (
        <Modal
          title={`${code} · ${s.t(
            expanded === "archive"
              ? "rfi.signed.previewArchive"
              : "rfi.signed.previewSigned",
          )}`}
          onClose={() => setExpanded(null)}
        >
          <div className="signed-page-preview-modal">
            <img
              src={expanded === "archive" ? archivePreview! : signedPreview!}
              alt={s.t(
                expanded === "archive"
                  ? "rfi.signed.previewArchiveAlt"
                  : "rfi.signed.previewSignedAlt",
                { code },
              )}
            />
          </div>
        </Modal>
      )}
    </>
  );
}

function outlookReplyPreview(value: unknown): {
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

function StepRow({
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

function ResultView({
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

function ReviewScreen({
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
                <div className="review-list">
                  {items.map((item) => (
                    <div className="review-row" key={item.selectionId}>
                      <button
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

function HistoryScreen({
  bridge,
  s,
  onBack,
}: {
  bridge: Bridge;
  s: Strings;
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
                        : row.opId === "rfi.process_scans_fast_covers"
                          ? "op.rfi.process_scans_fast_covers.title"
                          : "op.rfi.process_scans.title",
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
        <Modal title={s.t("action.undo")} onClose={() => setUndoRow(null)}>
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

function RecoveryScreen({
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

function AdminScreen({
  bridge,
  auth,
  s,
  onBack,
}: {
  bridge: Bridge;
  auth: AuthState;
  s: Strings;
  onBack: () => void;
}) {
  const [users, setUsers] = useState<AppUser[]>([]);
  const [busyId, setBusyId] = useState("");
  const [error, setError] = useState("");
  const load = useCallback(async () => {
    try {
      setUsers(await bridge.call("auth/listUsers"));
    } catch (reason) {
      setError(humanError(reason, s));
    }
  }, [bridge, s]);
  useEffect(() => {
    void load();
  }, [load]);
  const update = async (
    user: AppUser,
    patch: { allowed?: boolean; isAdmin?: boolean },
  ) => {
    setBusyId(user.id);
    try {
      await bridge.call("auth/setFlags", {
        userId: user.id,
        ...patch,
      });
      await load();
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusyId("");
    }
  };
  return (
    <div className="screen">
      <Header
        title={s.t("admin.title")}
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
            <h1>{s.t("admin.title")}</h1>
            <p>{s.t("admin.help")}</p>
          </div>
          <span className="safe-badge">
            <Shield /> Admin
          </span>
        </div>
        {users.length ? (
          <div className="admin-list">
            {users.map((user) => {
              const self = user.id === auth.session?.userId;
              return (
                <div className="admin-row" key={user.id}>
                  <span className="avatar">
                    {user.display.slice(0, 1).toUpperCase()}
                  </span>
                  <div className="admin-copy">
                    <strong>
                      {user.display}
                      {self && <em>{s.t("admin.you")}</em>}
                    </strong>
                    <small>
                      {user.email}
                      {!user.allowed && ` · ${s.t("admin.noAccess")}`}
                    </small>
                  </div>
                  <label className="mini-toggle">
                    {s.t("admin.allowed")}
                    <Switch
                      checked={user.allowed}
                      disabled={self || busyId === user.id}
                      onChange={(allowed) => void update(user, { allowed })}
                    />
                  </label>
                  <label className="mini-toggle">
                    {s.t("admin.admin")}
                    <Switch
                      checked={user.isAdmin}
                      disabled={self || busyId === user.id}
                      onChange={(isAdmin) => void update(user, { isAdmin })}
                    />
                  </label>
                </div>
              );
            })}
          </div>
        ) : (
          <EmptyState icon={<UserRoundCog />} title={s.t("admin.empty")} />
        )}
        {error && <Notice kind="error">{error}</Notice>}
      </main>
    </div>
  );
}

function SettingsScreen({
  bridge,
  auth,
  prefs,
  s,
  onPrefs,
  onAuth,
  onBack,
}: {
  bridge: Bridge;
  auth: AuthState;
  prefs: AppPrefs;
  s: Strings;
  onPrefs: (prefs: AppPrefs) => void;
  onAuth: (auth: AuthState) => void;
  onBack: () => void;
}) {
  const [username, setUsername] = useState(auth.user?.username ?? "");
  const [saved, setSaved] = useState("");
  const [error, setError] = useState("");
  const [diagnostics, setDiagnostics] = useState<SupportBundlePreview | null>(
    null,
  );
  const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [diagnosticsStatus, setDiagnosticsStatus] = useState("");
  const [replicaStatus, setReplicaStatus] =
    useState<ReplicaAutomationStatus | null>(null);
  const refreshReplicaStatus = useCallback(async () => {
    try {
      setReplicaStatus(await bridge.call("settings/replicaStatus"));
    } catch {
      // The preference remains usable when a read-only refresh is unavailable.
    }
  }, [bridge]);
  useEffect(() => {
    void refreshReplicaStatus();
    const timer = window.setInterval(() => void refreshReplicaStatus(), 5_000);
    return () => window.clearInterval(timer);
  }, [refreshReplicaStatus]);
  const update = async (patch: Partial<AppPrefs>) => {
    try {
      const next = await bridge.call(
        "settings/update",
        patch as Record<string, unknown>,
      );
      onPrefs(next);
      void refreshReplicaStatus();
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const saveUsername = async () => {
    setError("");
    try {
      onAuth(await bridge.call("auth/setUsername", { username }));
      setSaved(s.t("prefs.account.saved"));
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const previewDiagnostics = async () => {
    setError("");
    setDiagnosticsStatus("");
    setDiagnosticsBusy(true);
    try {
      setDiagnostics(await bridge.call("support/diagnostics"));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setDiagnosticsBusy(false);
    }
  };
  const copyDiagnostics = async () => {
    if (!diagnostics) return;
    setError("");
    setDiagnosticsStatus("");
    setDiagnosticsBusy(true);
    try {
      await bridge.call("support/copyDiagnostics", {
        content: diagnostics.content,
      });
      setDiagnosticsStatus(s.t("prefs.support.copied"));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setDiagnosticsBusy(false);
    }
  };
  const downloadDiagnostics = () => {
    if (!diagnostics) return;
    try {
      const blob = new Blob([diagnostics.content], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = diagnostics.fileName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      setDiagnosticsStatus(s.t("prefs.support.downloaded"));
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  return (
    <div className="screen">
      <Header
        title={s.t("prefs.title")}
        onBack={onBack}
        backLabel={s.t("action.back")}
      />
      <main className="settings-pane">
        {auth.signedIn && (
          <SettingsGroup title={s.t("prefs.account")} icon={<User />}>
            <div className="settings-account">
              <div>
                <strong>{auth.session?.email}</strong>
                <small>{s.t("prefs.account.emailFixed")}</small>
              </div>
              <input
                value={username}
                onChange={(event) => {
                  setUsername(event.target.value);
                  setSaved("");
                }}
                placeholder={s.t("auth.username")}
              />
              <Button
                variant="secondary"
                icon={<Save />}
                disabled={!usernamePattern.test(username)}
                onClick={() => void saveUsername()}
              >
                {s.t("action.save")}
              </Button>
            </div>
            {saved && <Notice kind="info">{saved}</Notice>}
          </SettingsGroup>
        )}
        <SettingsGroup title={s.t("prefs.language")} icon={<BadgeCheck />}>
          <ChoiceGrid
            value={prefs.locale ?? "system"}
            options={[
              ["system", s.t("prefs.lang.system")],
              ["tr", s.t("prefs.lang.tr")],
              ["en", s.t("prefs.lang.en")],
              ["ru", s.t("prefs.lang.ru")],
              ["az", s.t("prefs.lang.az")],
            ]}
            onChange={(locale) =>
              void update({
                locale:
                  locale === "system" ? null : (locale as AppPrefs["locale"]),
              })
            }
          />
        </SettingsGroup>
        <SettingsGroup title={s.t("prefs.theme")} icon={<SlidersHorizontal />}>
          <Segmented
            value={prefs.theme}
            options={(["system", "light", "dark"] as ThemePreference[]).map(
              (theme) => [theme, s.t(`prefs.theme.${theme}`)],
            )}
            onChange={(theme) =>
              void update({ theme: theme as ThemePreference })
            }
          />
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.font")}
          icon={<FileSearch />}
          help={s.t("prefs.font.help")}
        >
          <ChoiceGrid
            value={prefs.font}
            options={(["system", "sans", "serif", "mono"] as const).map(
              (font) => [font, s.t(`prefs.font.${font}`)],
            )}
            onChange={(font) => void update({ font: font as AppPrefs["font"] })}
          />
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.fontSize")}
          icon={<SlidersHorizontal />}
        >
          <div className="range-row">
            <span>80%</span>
            <input
              type="range"
              min="0.8"
              max="1.6"
              step="0.05"
              value={prefs.fontScale}
              onChange={(event) =>
                void update({ fontScale: Number(event.target.value) })
              }
            />
            <strong>{Math.round(prefs.fontScale * 100)}%</strong>
          </div>
          <div className="font-sample">{s.t("prefs.fontSize.sample")}</div>
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.density")}
          icon={<SlidersHorizontal />}
          help={s.t("prefs.density.help")}
        >
          <label className="toggle-row">
            <span>
              <strong>{s.t("prefs.density.compact")}</strong>
            </span>
            <Switch
              checked={prefs.compact}
              onChange={(compact) => void update({ compact })}
            />
          </label>
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.automation")}
          icon={<Monitor />}
          help={s.t("prefs.automation.help")}
        >
          <label className="toggle-row">
            <span>
              <strong>{s.t("prefs.startWithWindows")}</strong>
            </span>
            <Switch
              checked={prefs.startWithWindows}
              onChange={(startWithWindows) => void update({ startWithWindows })}
            />
          </label>
          <label className="toggle-row">
            <span>
              <strong>{s.t("prefs.replicaSyncEnabled")}</strong>
              <small>{s.t("prefs.replicaSyncEnabled.help")}</small>
              {replicaStatus && (
                <small>{s.t(`prefs.replica.status.${replicaStatus.status}`)}</small>
              )}
              {replicaStatus?.lastErrorCode && (
                <small>{s.t(replicaStatus.lastErrorCode)}</small>
              )}
            </span>
            <Switch
              checked={replicaStatus?.enabled ?? prefs.replicaSyncEnabled}
              onChange={(replicaSyncEnabled) =>
                void update({ replicaSyncEnabled })
              }
            />
          </label>
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.support.title")}
          icon={<Shield />}
          help={s.t("prefs.support.help")}
        >
          <div className="support-diagnostics">
            <p>{s.t("prefs.support.privacy")}</p>
            <div className="support-diagnostics-actions">
              <Button
                variant="secondary"
                icon={
                  diagnosticsBusy ? <LoaderCircle className="spin" /> : <Eye />
                }
                disabled={diagnosticsBusy}
                onClick={() => void previewDiagnostics()}
              >
                {s.t("prefs.support.preview")}
              </Button>
              {diagnostics && (
                <>
                  <Button
                    variant="secondary"
                    icon={<Copy />}
                    disabled={diagnosticsBusy}
                    onClick={() => void copyDiagnostics()}
                  >
                    {s.t("prefs.support.copy")}
                  </Button>
                  <Button
                    variant="secondary"
                    icon={<Download />}
                    disabled={diagnosticsBusy}
                    onClick={downloadDiagnostics}
                  >
                    {s.t("prefs.support.download")}
                  </Button>
                </>
              )}
            </div>
            {diagnostics && (
              <div className="support-diagnostics-preview">
                <small>
                  {diagnostics.fileName} ·{" "}
                  {s.t("prefs.support.size", {
                    count: diagnostics.byteLength,
                  })}
                </small>
                <pre aria-label={s.t("prefs.support.previewLabel")}>
                  {diagnostics.content}
                </pre>
              </div>
            )}
            {diagnosticsStatus && (
              <Notice kind="info">{diagnosticsStatus}</Notice>
            )}
          </div>
        </SettingsGroup>
        {error && <Notice kind="error">{error}</Notice>}
        <Button
          variant="danger"
          icon={<RotateCcw />}
          onClick={() =>
            void bridge
              .call("settings/reset")
              .then(onPrefs)
              .catch((reason: unknown) => setError(humanError(reason, s)))
          }
        >
          {s.t("prefs.reset")}
        </Button>
      </main>
    </div>
  );
}

function SettingsGroup({
  title,
  icon,
  help,
  children,
}: {
  title: string;
  icon: ReactNode;
  help?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-group">
      <div className="settings-title">
        <span>{icon}</span>
        <div>
          <h2>{title}</h2>
          {help && <p>{help}</p>}
        </div>
      </div>
      <div className="settings-content">{children}</div>
    </section>
  );
}

function ChoiceGrid({
  value,
  options,
  onChange,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="choice-grid">
      {options.map(([key, label]) => (
        <button
          type="button"
          key={key}
          className={value === key ? "selected" : ""}
          onClick={() => onChange(key)}
        >
          <span>{value === key && <Check />}</span>
          {label}
        </button>
      ))}
    </div>
  );
}

function formatDate(value: string, locale: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}
