import {
  ArrowRight,
  BadgeCheck,
  Check,
  Eye,
  EyeOff,
  Files,
  LoaderCircle,
  LogOut,
  RefreshCw,
  Settings,
  Shield,
  User,
} from "lucide-react";
import {
  type FormEvent,
  useEffect,
  useState,
} from "react";
import {
  Button,
  Centered,
  EmptyState,
  Field,
  Header,
  IconButton,
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
  type AuthState,
} from "../types";
import {
  humanError,
  usernamePattern,
} from "../helpers";

export function LoginScreen({
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

export function PendingScreen({
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
