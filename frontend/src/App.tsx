import {
  RefreshCw,
  XCircle,
} from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  Button,
  Centered,
  EmptyState,
} from "./components/ui";
import {
  loadLocale,
  makeStrings,
  systemLocale,
} from "./i18n";
import {
  getBridge,
} from "./lib/bridge";
import {
} from "./lib/platform";
import {
  type AppPrefs,
  type AuthState,
  type Bootstrap,
} from "./types";
import {
  Route,
  applyAppearance,
} from "./helpers";
import { AppFrame, Splash } from "./AppFrame";
import {
  LoginScreen,
  PendingScreen,
} from "./screens/AuthScreens";
import {
  HomeScreen,
} from "./screens/HomeScreen";
import {
  OperationScreen,
} from "./screens/OperationScreen";
import {
  SettingsScreen,
} from "./screens/SettingsScreen";

// Rarely opened routes load on first visit to keep the initial bundle smaller.
const ReviewScreen = lazy(() =>
  import("./screens/ReviewScreen").then((module) => ({ default: module.ReviewScreen })),
);
const QueueScreen = lazy(() =>
  import("./screens/QueueScreen").then((module) => ({ default: module.QueueScreen })),
);
const HistoryScreen = lazy(() =>
  import("./screens/HistoryScreen").then((module) => ({ default: module.HistoryScreen })),
);
const RecoveryScreen = lazy(() =>
  import("./screens/RecoveryScreen").then((module) => ({ default: module.RecoveryScreen })),
);
const AdminScreen = lazy(() =>
  import("./screens/AdminScreen").then((module) => ({ default: module.AdminScreen })),
);

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
      await loadLocale(value.prefs.locale ?? systemLocale());
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
    void loadLocale(prefs.locale ?? systemLocale()).then(() =>
      setBootstrap((current) => (current ? { ...current, prefs } : current)),
    );
  };

  if (route.name === "settings") {
    const settingsScreen = (
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
    return bootstrap.auth.signedIn && bootstrap.auth.allowed ? (
      <AppFrame
        bridge={bridge}
        routeName={route.name}
        isAdmin={bootstrap.auth.isAdmin}
        user={bootstrap.auth.user}
        s={s}
        onNavigate={setRoute}
      >
        {settingsScreen}
      </AppFrame>
    ) : (
      settingsScreen
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

  const page = (() => {
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
            onReview={(destination) =>
              setRoute({ name: "review", destination })
            }
            onPrepareRfiReply={() => {}}
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
      case "queue":
        return (
          <QueueScreen
            bridge={bridge}
            operations={bootstrap.operations}
            s={s}
            onBack={() => setRoute({ name: "home" })}
            onOpenOperation={(operation) =>
              setRoute({ name: "operation", operation })
            }
            onOpenPlan={(operation, plan, values) =>
              setRoute({
                name: "operation",
                operation,
                initialValues: values,
                initialPlan: plan,
              })
            }
          />
        );
      case "history":
        return (
          <HistoryScreen
            bridge={bridge}
            s={s}
            operations={bootstrap.operations}
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
  })();

  return (
    <AppFrame
      bridge={bridge}
      routeName={route.name}
      isAdmin={bootstrap.auth.isAdmin}
      user={bootstrap.auth.user}
      s={s}
      onNavigate={setRoute}
    >
      <Suspense fallback={null}>{page}</Suspense>
    </AppFrame>
  );
}
