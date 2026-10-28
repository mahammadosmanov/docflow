import {
  BadgeCheck,
  Copy,
  Download,
  Eye,
  FileSearch,
  LoaderCircle,
  Monitor,
  RotateCcw,
  Save,
  Shield,
  SlidersHorizontal,
  User,
} from "lucide-react";
import {
  useState,
} from "react";
import {
  Button,
  Header,
  Notice,
  Segmented,
  Switch,
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
  type ThemePreference,
} from "../types";
import {
  humanError,
  usernamePattern,
} from "../helpers";

import { useSupportDiagnostics } from "./useSupportDiagnostics";

import { useReplicaStatus } from "./useReplicaStatus";

import { ChoiceGrid, SettingsGroup } from "./SettingsParts";

export function SettingsScreen({
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
  const { replicaStatus, refreshReplicaStatus } = useReplicaStatus(bridge);
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
  const {
    diagnostics,
    diagnosticsBusy,
    diagnosticsStatus,
    previewDiagnostics,
    copyDiagnostics,
    saveActivityLog,
    downloadDiagnostics,
  } = useSupportDiagnostics({ bridge, s, setError });
  return (
    <div className="screen">
      <Header
        title={s.t("prefs.title")}
        titleAs="h1"
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
                aria-label={s.t("auth.username")}
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
              aria-label={s.t("prefs.fontSize")}
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
              ariaLabel={s.t("prefs.density.compact")}
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
              ariaLabel={s.t("prefs.startWithWindows")}
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
              ariaLabel={s.t("prefs.replicaSyncEnabled")}
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
              <Button
                variant="secondary"
                icon={<Download />}
                disabled={diagnosticsBusy}
                onClick={() => void saveActivityLog()}
              >
                {s.t("prefs.support.activityLog")}
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
