import {
  RefreshCw,
  Shield,
  UserRoundCog,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useState,
} from "react";
import {
  Header,
  IconButton,
  Switch,
} from "../components/ui";
import {
  EmptyState,
  Notice,
} from "../components/feedback";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  type AppUser,
  type AuthState,
} from "../types";
import {
  humanError,
} from "../helpers";

export function AdminScreen({
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
                      ariaLabel={`${s.t("admin.allowed")}: ${user.display}`}
                      onChange={(allowed) => void update(user, { allowed })}
                    />
                  </label>
                  <label className="mini-toggle">
                    {s.t("admin.admin")}
                    <Switch
                      checked={user.isAdmin}
                      disabled={self || busyId === user.id}
                      ariaLabel={`${s.t("admin.admin")}: ${user.display}`}
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
