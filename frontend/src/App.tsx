import {
  ArrowLeft,
  ArrowRight,
  BadgeCheck,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleAlert,
  Copy,
  Eye,
  EyeOff,
  FileCheck2,
  FileSearch,
  Files,
  FolderOpen,
  History,
  LoaderCircle,
  LogOut,
  Play,
  Plus,
  Printer,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  Shield,
  SlidersHorizontal,
  TableProperties,
  Trash2,
  Undo2,
  User,
  UserRoundCog,
  X,
  XCircle,
} from "lucide-react";
import {
  type ButtonHTMLAttributes,
  type FormEvent,
  type InputHTMLAttributes,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import { makeStrings, type Strings } from "./i18n";
import { getBridge, type Bridge } from "./lib/bridge";
import {
  chooseFile,
  chooseFiles,
  chooseFolder,
  revealPath,
} from "./lib/platform";
import type {
  AppPrefs,
  AppUser,
  AuthState,
  Bootstrap,
  HistoryPayload,
  HistoryRow,
  OperationDef,
  OperationResult,
  ParamDef,
  Plan,
  ProgressEvent,
  ReviewAction,
  ReviewDecision,
  ReviewItem,
  Step,
  ThemePreference,
} from "./types";

type Route =
  | { name: "home" }
  | { name: "operation"; operation: OperationDef }
  | { name: "review"; destination: string }
  | { name: "history" }
  | { name: "admin" }
  | { name: "settings"; back: "home" | "login" | "pending" };

const usernamePattern = /^[a-zA-Z0-9._-]{3,24}$/;

function humanError(error: unknown, s: Strings): string {
  const raw = error instanceof Error ? error.message : String(error);
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

function applyAppearance(prefs: AppPrefs): void {
  const root = document.documentElement;
  root.dataset.theme = prefs.theme;
  root.dataset.font = prefs.font;
  root.dataset.compact = String(prefs.compact);
  root.style.fontSize = `${prefs.fontScale * 16}px`;
}

export function App() {
  const bridge = useMemo(() => getBridge(), []);
  const [bootstrap, setBootstrap] = useState<Bootstrap | null>(null);
  const [fatal, setFatal] = useState("");
  const [route, setRoute] = useState<Route>({ name: "home" });

  const load = useCallback(async () => {
    setFatal("");
    try {
      const value = await bridge.call<Bootstrap>("app/bootstrap");
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
          title="DocFlow başlatılamadı"
          message={fatal}
          action={
            <Button onClick={() => void load()} icon={<RefreshCw />}>
              Tekrar dene
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
          bridge={bridge}
          operation={route.operation}
          s={s}
          onBack={() => setRoute({ name: "home" })}
          onReview={(destination) =>
            setRoute({ name: "review", destination })
          }
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
        setAvailable(
          await bridge.call<boolean>("auth/usernameAvailable", { username }),
        );
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
      ? identifier.trim().length > 0
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
        const auth = await bridge.call<AuthState>("auth/signIn", {
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
                label={s.t("auth.identifier")}
                help={s.t("auth.identifierHelp")}
                autoComplete="username"
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
              autoComplete={
                mode === "in" ? "current-password" : "new-password"
              }
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
            icon={
              busy ? <LoaderCircle className="spin" /> : <ArrowRight />
            }
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
  const run = async (method: string) => {
    setBusy(true);
    setError("");
    try {
      onAuth(await bridge.call<AuthState>(method));
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
              onClick={() => void run("auth/signOut")}
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
              onClick={() => void run("auth/loadMe")}
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
  navigate,
}: {
  bridge: Bridge;
  bootstrap: Bootstrap;
  s: Strings;
  onAuth: (auth: AuthState) => void;
  navigate: (route: Route) => void;
}) {
  const who =
    bootstrap.auth.user?.display || bootstrap.auth.session?.email || "";
  const [reviewBusy, setReviewBusy] = useState(false);
  const [shortcutError, setShortcutError] = useState("");
  const signOut = async () =>
    onAuth(await bridge.call<AuthState>("auth/signOut"));
  const openReview = async () => {
    setReviewBusy(true);
    setShortcutError("");
    try {
      const operation =
        bootstrap.operations.find((item) => item.id === "rfi.process_scans") ??
        bootstrap.operations[0];
      const saved = operation
        ? await bridge.call<Record<string, unknown>>("settings/params", {
            opId: operation.id,
          })
        : {};
      let destination = String(saved.dest ?? "").trim();
      if (!destination) destination = (await chooseFolder()) ?? "";
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
            <Shield /> Preview-first
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
  s,
  onBack,
  onReview,
}: {
  bridge: Bridge;
  operation: OperationDef;
  s: Strings;
  onBack: () => void;
  onReview: (destination: string) => void;
}) {
  const defaults = Object.fromEntries(
    operation.schema.map((item) => [item.name, item.default]),
  );
  const [values, setValues] =
    useState<Record<string, unknown>>(defaults);
  const [stage, setStage] = useState<
    "form" | "previewing" | "preview" | "applying" | "result"
  >("form");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [result, setResult] = useState<OperationResult | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [error, setError] = useState("");
  const [editorRows, setEditorRows] = useState<Record<string, unknown>[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  const [copyingSheet, setCopyingSheet] = useState<string | null>(null);

  useEffect(() => {
    void bridge
      .call<Record<string, unknown>>("settings/params", {
        opId: operation.id,
      })
      .then((saved) => {
        const known = new Set(
          operation.schema.map((item) => item.name),
        );
        setValues((current) => ({
          ...current,
          ...Object.fromEntries(
            Object.entries(saved).filter(([key]) => known.has(key)),
          ),
        }));
      });
  }, [bridge, operation]);

  const preview = async () => {
    setError("");
    setStage("previewing");
    try {
      await bridge.call("settings/saveParams", {
        opId: operation.id,
        values: Object.fromEntries(
          Object.entries(values).filter(([name]) => {
            const param = operation.schema.find((item) => item.name === name);
            return param?.kind !== "longText" && param?.kind !== "secret";
          }),
        ),
      });
      const next = await bridge.call<Plan>(
        "engine/preview",
        { opId: operation.id, values },
        setProgress,
      );
      setPlan(next);
      const editor = next.meta?.editor;
      const rows =
        editor && typeof editor === "object" && Array.isArray((editor as Record<string, unknown>).rows)
          ? ((editor as Record<string, unknown>).rows as Record<string, unknown>[])
          : [];
      setEditorRows(rows.map((row) => ({ ...row })));
      setEditorDirty(false);
      setStage("preview");
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("form");
    }
  };
  const apply = async () => {
    if (!plan) return;
    setError("");
    setProgress(null);
    try {
      let approvedPlan = plan;
      if (editorDirty) {
        const previousWarnings = new Set(
          plan.warnings.map((warning) => `${warning.key}\u0000${warning.subject}`),
        );
        const revised = await bridge.call<Plan>("engine/revise", {
          planId: plan.planId,
          rows: editorRows,
        });
        setPlan(revised);
        const revisedEditor = revised.meta?.editor as
          | Record<string, unknown>
          | undefined;
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
        await bridge.call<OperationResult>(
          "engine/apply",
          { opId: operation.id, planId: approvedPlan.planId },
          setProgress,
        ),
      );
      setStage("result");
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("preview");
    }
  };
  const copyTable = async (sheet?: string) => {
    if (!plan || copyingSheet !== null) return;
    setCopyingSheet(sheet ?? "");
    setError("");
    try {
      const payload = await bridge.call<{ count: number }>("engine/copyTable", {
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
  const currentStep =
    stage === "form"
      ? 0
      : stage === "previewing" || stage === "preview"
        ? 1
        : 2;

  return (
    <div className="screen">
      <Header title={s.t(operation.titleKey)} onBack={onBack}>
        <StepIndicator
          current={currentStep}
          labels={[
            s.t("step.form"),
            s.t("step.preview"),
            s.t("step.apply"),
          ]}
        />
      </Header>
      <main
        className={`work-pane ${
          stage === "preview" && plan?.meta?.editor ? "editor-pane" : ""
        }`}
      >
        {stage === "form" && (
          <div className="flow-layout">
            <section className="content-card">
              <div className="card-title">
                <SlidersHorizontal />
                <div>
                  <h2>{s.t("step.form")}</h2>
                  <p>{s.t(operation.descriptionKey)}</p>
                </div>
              </div>
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
                    param={param}
                    value={values[param.name]}
                    values={values}
                    s={s}
                    onChange={(value) =>
                      setValues((current) => ({
                        ...current,
                        [param.name]: value,
                      }))
                    }
                  />
                ))}
              </div>
            </section>
            {error && <Notice kind="error">{error}</Notice>}
            <ActionBar
              left={operation.schema.some((param) => param.name === "dest") ? (
                <Button
                  variant="ghost"
                  icon={<FolderOpen />}
                  onClick={() => {
                    const destination = String(values.dest ?? "").trim();
                    if (!destination)
                      setError(s.t("error.param.required.dest"));
                    else onReview(destination);
                  }}
                >
                  {s.t("action.review")}
                </Button>
              ) : undefined}
              right={
                <Button icon={<Eye />} onClick={() => void preview()}>
                  {s.t("action.preview")}
                </Button>
              }
            />
          </div>
        )}
        {stage === "previewing" && (
          <BusyState label={s.t("action.preview")} progress={progress} />
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
          <BusyState label={s.t("plan.applying")} progress={progress} />
        )}
        {stage === "result" && result && (
          <ResultView
            result={result}
            s={s}
            error={error}
            onDone={onBack}
            onUndo={() => void undo()}
            copySheets={copySheetsFromPlan(plan)}
            copyEnabled={Boolean(
              (plan?.meta?.editor as Record<string, unknown> | undefined)
                ?.copyEnabled,
            )}
            copyingSheet={copyingSheet}
            onCopy={(sheet) => void copyTable(sheet)}
          />
        )}
      </main>
    </div>
  );
}

function SchemaField({
  param,
  value,
  values,
  s,
  onChange,
}: {
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
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <select
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        >
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
    const paths = Array.isArray(value)
      ? value.map((item) => String(item))
      : [];
    return (
      <div className="field">
        <span>{s.t(param.labelKey)}</span>
        <div className="multi-file-actions">
          <Button
            type="button"
            variant="secondary"
            icon={<Plus />}
            onClick={() =>
              void chooseFiles().then((selected) =>
                onChange([...new Set([...paths, ...selected])]),
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
              <div className="selected-file" key={`${path}-${index}`}>
                <span title={path}>{path.split(/[\\/]/).pop()}</span>
                <button
                  type="button"
                  title={s.t("action.remove")}
                  onClick={() =>
                    onChange(paths.filter((_, itemIndex) => itemIndex !== index))
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
              <div className={`sheet-count ${count > 0 ? "active" : ""}`} key={choice}>
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
      <div className="path-input">
        <input
          type={
            param.kind === "int"
              ? "number"
              : param.kind === "secret"
                ? "password"
                : "text"
          }
          value={String(value ?? "")}
          placeholder={browse ? s.t(`param.hint.${param.kind}`) : ""}
          onChange={(event) =>
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
              void (param.kind === "folder"
                ? chooseFolder()
                : chooseFile()
              ).then((path) => {
                if (path) onChange(path);
              })
            }
          >
            {s.t("action.browse")}
          </Button>
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
};

function EditablePlanTable({
  editor,
  rows,
  s,
  onChange,
}: {
  editor: Record<string, unknown>;
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
    const options = (values as Record<string, unknown>)[String(row[dependency] ?? "")];
    return Array.isArray(options) ? options.map(String) : [];
  };

  const update = (index: number, key: string, value: string) => {
    const next = rows.map((row) => ({ ...row }));
    const row = next[index];
    row[key] = value;

    const autoFill = editor.autoFill;
    if (autoFill && typeof autoFill === "object") {
      const config = autoFill as Record<string, unknown>;
      if (config.field === key && config.values && typeof config.values === "object") {
        const preset = (config.values as Record<string, unknown>)[value];
        if (preset && typeof preset === "object") Object.assign(row, preset);
      }
    }

    if (Array.isArray(editor.regexFill)) {
      for (const rawRule of editor.regexFill) {
        if (!rawRule || typeof rawRule !== "object") continue;
        const rule = rawRule as Record<string, unknown>;
        if (rule.source !== key) continue;
        try {
          if (new RegExp(String(rule.pattern), "i").test(value)) {
            row[String(rule.target)] = String(rule.value ?? "");
          }
        } catch {
          // Invalid server-side presentation hints never block row editing.
        }
      }
    }
    onChange(next);
  };

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

  return (
    <section className="editor-card">
      <div className="editor-heading">
        <span><TableProperties /></span>
        <div>
          <strong>{s.t(String(editor.titleKey ?? "rfi.editor.title"))}</strong>
          <small>{s.t("rfi.editor.rowCount", { count: rows.length })}</small>
        </div>
      </div>
      <div className="editor-scroll">
        <table className="editor-table">
          <thead>
            <tr>
              {columns.map((column) => (
                <th key={column.key} style={{ minWidth: column.width ?? 120 }}>
                  {s.t(column.labelKey)}
                </th>
              ))}
              {editor.duplicateEnabled === true && <th aria-label="actions" />}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, rowIndex) => {
              const sheet = String(row._sheet ?? "");
              const groupCount = rows.filter(
                (item) => item[groupField] === row[groupField],
              ).length;
              return (
                <tr key={String(row._rowId ?? `${row[groupField]}-${rowIndex}`)}>
                  {columns.map((column) => {
                    if (column.sheets && !column.sheets.includes(sheet)) {
                      return <td key={column.key} className="not-applicable">—</td>;
                    }
                    const text = String(row[column.key] ?? "");
                    const choices = choicesFor(column, row);
                    const suggestions = Array.isArray(autocomplete[column.key])
                      ? (autocomplete[column.key] as unknown[]).map(String)
                      : [];
                    const listId = `editor-${rowIndex}-${column.key}`;
                    return (
                      <td key={column.key}>
                        {column.kind === "readonly" ? (
                          <span className="readonly-cell" title={text}>{text}</span>
                        ) : column.kind === "choice" ? (
                          <select
                            value={text}
                            onChange={(event) =>
                              update(rowIndex, column.key, event.target.value)
                            }
                          >
                            {choices.map((choice) => (
                              <option key={choice} value={choice}>{choice}</option>
                            ))}
                          </select>
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
                  {editor.duplicateEnabled === true && (
                    <td className="editor-row-actions">
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
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function PlanView({
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
  const moves = plan.steps.filter((step) => step.kind !== "mkdir").length;
  const editor =
    plan.meta?.editor && typeof plan.meta.editor === "object"
      ? (plan.meta.editor as Record<string, unknown>)
      : null;
  return (
    <div className="flow-layout">
      {plan.warnings.length > 0 && (
        <Notice kind="warning" title={s.t("ui.warnings")}>
          {plan.warnings.map((warning) => (
            <div key={`${warning.key}-${warning.subject}`}>
              {warning.subject && <strong>{warning.subject}: </strong>}
              {s.t(warning.key)}
            </div>
          ))}
        </Notice>
      )}
      {editor && editorRows && onEditorRowsChange ? (
        <EditablePlanTable
          editor={editor}
          rows={editorRows}
          s={s}
          onChange={onEditorRowsChange}
        />
      ) : <section className="plan-section">
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
          <EmptyState
            icon={<FileCheck2 />}
            title={s.t("plan.empty")}
          />
        )}
      </section>}
      {error && <Notice kind="error">{error}</Notice>}
      <ActionBar
        left={
          <Button variant="ghost" onClick={onCancel}>
            {s.t("action.cancel")}
          </Button>
        }
        right={
          <Button
            icon={<Play />}
            disabled={!plan.steps.length}
            onClick={onApply}
          >
            {s.t("action.apply")}
          </Button>
        }
      />
    </div>
  );
}

function StepRow({ step, s }: { step: Step; s: Strings }) {
  const source = step.src?.split(/[\\/]/).pop();
  const destination = step.dst?.split(/[\\/]/).pop();
  return (
    <div className="step-row">
      <span className="step-kind">
        {step.kind === "print" ? (
          <Printer />
        ) : step.kind === "workbook_update" ? (
          <TableProperties />
        ) : step.kind === "pdf_first_page_replace" ? (
          <Files />
        ) : step.kind === "copy" ? (
          <Copy />
        ) : step.kind === "mkdir" ? (
          <FolderOpen />
        ) : (
          <ArrowRight />
        )}
      </span>
      <div className="step-copy">
        <strong>{step.noteKey ? s.t(step.noteKey) : step.kind}</strong>
        <span title={step.src ?? ""}>{source || "—"}</span>
        {step.dst && (
          <span className="destination" title={step.dst}>
            → {destination}
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
}) {
  const success = result.failed === 0;
  return (
    <div className="result-card">
      <span className={`result-icon ${success ? "success" : "failure"}`}>
        {success ? <CheckCircle2 /> : <CircleAlert />}
      </span>
      <p className="eyebrow">
        {success ? "DOCFLOW" : s.t("ui.warnings")}
      </p>
      <h1>{s.t("result.applied", { count: result.applied })}</h1>
      <p className="muted">
        {s.t("result.failed", { count: result.failed })}
      </p>
      {result.cloudSynced !== undefined && (
        <span className="safe-badge">
          {result.cloudSynced ? <Check /> : <History />}
          {result.cloudSynced
            ? s.t("history.cloud")
            : s.t("history.local")}
        </span>
      )}
      {result.errors.length > 0 && (
        <Notice kind="warning">
          {result.errors
            .map((item) => `${item.subject} ${s.t(item.key)}`)
            .join("\n")}
        </Notice>
      )}
      {error && <Notice kind="error">{error}</Notice>}
      {onCopy && (copySheets.length > 0 || copyEnabled) && (
        <div className="copy-actions">
          {copySheets.map((sheet) => (
            <Button
              key={sheet}
              variant="secondary"
              icon={
                copyingSheet === sheet
                  ? <LoaderCircle className="spin" />
                  : <TableProperties />
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
                copyingSheet !== null
                  ? <LoaderCircle className="spin" />
                  : <TableProperties />
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
        <Button
          variant="secondary"
          icon={<Undo2 />}
          disabled={!result.journalId}
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
  destination: string;
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
      const rows = await bridge.call<ReviewItem[]>("engine/reviewScan", {
        destination,
      });
      setItems(rows);
      setActions(
        Object.fromEntries(rows.map((item) => [item.path, "keep"])),
      );
      setCodes(
        Object.fromEntries(rows.map((item) => [item.path, item.code])),
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
    .filter((item) => actions[item.path] !== "keep")
    .map((item) => ({
      path: item.path,
      action: actions[item.path] as ReviewDecision["action"],
      code: codes[item.path] ?? "",
    }));

  const preview = async () => {
    setBusy(true);
    setError("");
    try {
      setPlan(
        await bridge.call<Plan>("engine/reviewPreview", {
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
        await bridge.call<OperationResult>("engine/apply", {
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
      <Header title={s.t("review.title")} onBack={onBack} />
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
          <BusyState label={s.t("action.review")} />
        ) : (
          <div className="flow-layout">
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
                  <div className="review-row" key={item.path}>
                    <button
                      className="file-button"
                      title={item.path}
                      onClick={() => void revealPath(item.path)}
                    >
                      <FileSearch />
                      <span>
                        <strong>{item.name}</strong>
                        <small>{item.path}</small>
                      </span>
                    </button>
                    <input
                      aria-label={`${item.name} code`}
                      value={codes[item.path] ?? ""}
                      placeholder={s.t("review.noCode")}
                      disabled={actions[item.path] !== "put"}
                      onChange={(event) =>
                        setCodes((current) => ({
                          ...current,
                          [item.path]: event.target.value,
                        }))
                      }
                    />
                    <Segmented
                      value={actions[item.path] ?? "keep"}
                      options={[
                        ["keep", s.t("review.keep")],
                        ["put", s.t("review.put")],
                        ["delete", s.t("review.delete")],
                      ]}
                      onChange={(action) =>
                        setActions((current) => ({
                          ...current,
                          [item.path]: action as ReviewAction,
                        }))
                      }
                      small
                    />
                    {item.targetExists &&
                      actions[item.path] === "put" && (
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
      setPayload(
        await bridge.call<HistoryPayload>("history/list", { limit: 50 }),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  }, [bridge, s]);
  useEffect(() => {
    void load();
  }, [load]);

  const undo = async (row: HistoryRow, destination?: string) => {
    setUndoRow(null);
    setBusy(true);
    try {
      await bridge.call("engine/undo", {
        journalId: row.journalId,
        steps: row.steps,
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
        trailing={
          <IconButton
            label={s.t("action.retry")}
            onClick={() => void load()}
          >
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
          <BusyState label={s.t("history.title")} />
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
                void chooseFolder().then((folder) => {
                  if (folder) return undo(undoRow, folder);
                })
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
      setUsers(await bridge.call<AppUser[]>("auth/listUsers"));
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
        trailing={
          <IconButton
            label={s.t("action.retry")}
            onClick={() => void load()}
          >
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
                      onChange={(allowed) =>
                        void update(user, { allowed })
                      }
                    />
                  </label>
                  <label className="mini-toggle">
                    {s.t("admin.admin")}
                    <Switch
                      checked={user.isAdmin}
                      disabled={self || busyId === user.id}
                      onChange={(isAdmin) =>
                        void update(user, { isAdmin })
                      }
                    />
                  </label>
                </div>
              );
            })}
          </div>
        ) : (
          <EmptyState
            icon={<UserRoundCog />}
            title={s.t("admin.empty")}
          />
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
  const update = async (patch: Partial<AppPrefs>) => {
    try {
      onPrefs(
        await bridge.call<AppPrefs>(
          "settings/update",
          patch as Record<string, unknown>,
        ),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  const saveUsername = async () => {
    setError("");
    try {
      onAuth(
        await bridge.call<AuthState>("auth/setUsername", { username }),
      );
      setSaved(s.t("prefs.account.saved"));
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };
  return (
    <div className="screen">
      <Header title={s.t("prefs.title")} onBack={onBack} />
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
        <SettingsGroup
          title={s.t("prefs.language")}
          icon={<BadgeCheck />}
        >
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
                  locale === "system"
                    ? null
                    : (locale as AppPrefs["locale"]),
              })
            }
          />
        </SettingsGroup>
        <SettingsGroup
          title={s.t("prefs.theme")}
          icon={<SlidersHorizontal />}
        >
          <Segmented
            value={prefs.theme}
            options={(
              ["system", "light", "dark"] as ThemePreference[]
            ).map((theme) => [theme, s.t(`prefs.theme.${theme}`)])}
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
            onChange={(font) =>
              void update({ font: font as AppPrefs["font"] })
            }
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
          <div className="font-sample">
            {s.t("prefs.fontSize.sample")}
          </div>
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
        {error && <Notice kind="error">{error}</Notice>}
        <Button
          variant="danger"
          icon={<RotateCcw />}
          onClick={() =>
            void bridge
              .call<AppPrefs>("settings/reset")
              .then(onPrefs)
              .catch((reason: unknown) =>
                setError(humanError(reason, s)),
              )
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

function Header({
  title,
  onBack,
  trailing,
  children,
}: {
  title?: string;
  onBack?: () => void;
  trailing?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className={`app-header ${children ? "with-steps" : ""}`}>
      <div className="header-main">
        <div className="header-left">
          {onBack && (
            <IconButton label="Back" onClick={onBack}>
              <ArrowLeft />
            </IconButton>
          )}
          <h1>{title}</h1>
        </div>
        <div className="header-actions">{trailing}</div>
      </div>
      {children}
    </header>
  );
}

function StepIndicator({
  current,
  labels,
}: {
  current: number;
  labels: string[];
}) {
  return (
    <div className="step-indicator" role="list">
      {labels.map((label, index) => (
        <div
          role="listitem"
          aria-current={index === current ? "step" : undefined}
          className={`${index === current ? "current" : ""} ${
            index < current ? "done" : ""
          }`}
          key={label}
        >
          <span>{index < current ? <Check /> : index + 1}</span>
          <small>{label}</small>
        </div>
      ))}
    </div>
  );
}

function Button({
  children,
  icon,
  variant = "primary",
  wide,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  wide?: boolean;
}) {
  return (
    <button
      className={`button ${variant} ${wide ? "wide" : ""}`}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

function IconButton({
  label,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="icon-button"
      title={label}
      aria-label={label}
      {...props}
    >
      {children}
    </button>
  );
}

function Field({
  label,
  help,
  icon,
  suffix,
  invalid,
  onChange,
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, "onChange"> & {
  label: string;
  help?: string;
  icon?: ReactNode;
  suffix?: ReactNode;
  invalid?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className={`field ${invalid ? "invalid" : ""}`}>
      <span>{label}</span>
      <div className="input-shell">
        {icon}
        <input
          aria-label={label}
          {...props}
          onChange={(event) => onChange(event.target.value)}
        />
        {suffix}
      </div>
      {help && <small>{help}</small>}
    </label>
  );
}

function Switch({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`switch ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

function Segmented({
  value,
  options,
  onChange,
  small,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
  small?: boolean;
}) {
  return (
    <div className={`segmented ${small ? "small" : ""}`}>
      {options.map(([key, label]) => (
        <button
          type="button"
          className={key === value ? "selected" : ""}
          key={key}
          onClick={() => onChange(key)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function Notice({
  kind,
  title,
  children,
}: {
  kind: "error" | "warning" | "info";
  title?: string;
  children: ReactNode;
}) {
  return (
    <div className={`notice ${kind}`}>
      {kind === "info" ? <CheckCircle2 /> : <CircleAlert />}
      <div>
        {title && <strong>{title}</strong>}
        <div>{children}</div>
      </div>
    </div>
  );
}

function EmptyState({
  icon,
  title,
  message,
  action,
}: {
  icon: ReactNode;
  title: string;
  message?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <span>{icon}</span>
      <h2>{title}</h2>
      {message && <p>{message}</p>}
      {action}
    </div>
  );
}

function BusyState({
  label,
  progress,
}: {
  label: string;
  progress?: ProgressEvent | null;
}) {
  const fraction =
    progress && progress.total > 0 ? progress.done / progress.total : 0;
  return (
    <div className="busy-state">
      <LoaderCircle className="spin" />
      <h2>{label}</h2>
      <div className="progress">
        <span style={{ width: fraction ? `${fraction * 100}%` : "35%" }} />
      </div>
      {progress && (
        <small>
          {progress.done} / {progress.total}
          {progress.subject ? ` · ${progress.subject}` : ""}
        </small>
      )}
    </div>
  );
}

function ActionBar({
  left,
  right,
}: {
  left: ReactNode;
  right: ReactNode;
}) {
  return (
    <div className="action-bar">
      <div>{left}</div>
      <div>{right}</div>
    </div>
  );
}

function Centered({ children }: { children: ReactNode }) {
  return <main className="centered">{children}</main>;
}

function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-modal="true">
        <div className="modal-header">
          <h2>{title}</h2>
          <IconButton label="Close" onClick={onClose}>
            <X />
          </IconButton>
        </div>
        {children}
      </div>
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
