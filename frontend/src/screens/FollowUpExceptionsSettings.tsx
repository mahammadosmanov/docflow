import { FileSearch } from "lucide-react";
import { useState } from "react";
import { Notice } from "../components/feedback";
import { Button } from "../components/ui";
import { humanError } from "../helpers";
import type { Strings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { CommandParams, CommandResult } from "../lib/commands";
import { SettingsGroup } from "./SettingsParts";

export function FollowUpExceptionsSettings({ bridge, s }: { bridge: Bridge; s: Strings }) {
  type Change = CommandParams<"settings/changeFollowUpException">["change"];
  const [list, setList] = useState<CommandResult<"settings/followUpExceptions">>();
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");

  const run = async (change?: Change) => {
    setBusy(true);
    setSaved(false);
    setError("");
    try {
      if (change) {
        await bridge.call("settings/changeFollowUpException", { code: code.trim(), change });
        setSaved(true);
        setCode("");
      }
      setList(await bridge.call("settings/followUpExceptions"));
    } catch (reason) {
      setList(undefined);
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsGroup
      title={s.t("prefs.followUpExceptions.title")}
      help={s.t("prefs.followUpExceptions.help")}
      icon={<FileSearch />}
    >
      <div className="support-diagnostics-actions">
        <Button variant="secondary" disabled={busy} onClick={() => void run()}>
          {s.t("prefs.followUpExceptions.load")}
        </Button>
      </div>
      {list && (
        <>
          {list.shared === null ? (
            <Notice kind="warning">{s.t("error.rfi.followUp.exceptionCloudUnavailable")}</Notice>
          ) : (
            <label className="field">
              <span>{s.t("prefs.followUpExceptions.shared")}</span>
              <textarea readOnly value={list.shared.join("\n")} />
            </label>
          )}
          {list.local.length > 0 && (
            <label className="field">
              <span>{s.t("prefs.followUpExceptions.local")}</span>
              <textarea readOnly value={list.local.join("\n")} />
            </label>
          )}
          {(list.shared !== null || list.local.length > 0) && (
            <>
              <label className="settings-account">
                <span>{s.t("prefs.followUpExceptions.code")}</span>
                <input
                  disabled={busy}
                  maxLength={64}
                  placeholder="CIV-1374"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                />
              </label>
              <div className="support-diagnostics-actions">
                {list.shared !== null && (
                  <>
                    <Button disabled={busy || !code.trim()} onClick={() => void run("addShared")}>
                      {s.t("prefs.followUpExceptions.add")}
                    </Button>
                    <Button variant="secondary" disabled={busy || !code.trim()} onClick={() => void run("removeShared")}>
                      {s.t("prefs.followUpExceptions.remove")}
                    </Button>
                  </>
                )}
                {list.local.length > 0 && (
                  <Button variant="secondary" disabled={busy || !code.trim()} onClick={() => void run("removeLocal")}>
                    {s.t("prefs.followUpExceptions.removeLocal")}
                  </Button>
                )}
              </div>
            </>
          )}
        </>
      )}
      {saved && <Notice kind="info">{s.t("prefs.followUpExceptions.saved")}</Notice>}
      {error && <Notice kind="error">{error}</Notice>}
    </SettingsGroup>
  );
}
