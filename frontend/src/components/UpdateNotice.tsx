import { useEffect, useState } from "react";
import type { Strings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { ProgressEvent, UpdateCheckResult } from "../types";

export function UpdateNotice({ bridge, s }: { bridge: Bridge; s: Strings }) {
  const [status, setStatus] = useState<UpdateCheckResult | null>(null);
  const [checking, setChecking] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [download, setDownload] = useState<ProgressEvent | null>(null);

  const check = async () => {
    setChecking(true);
    setError(false);
    try {
      setStatus(await bridge.call("update/check"));
    } catch {
      // Silent on startup: a failed background check must not nag the user.
      setStatus(null);
    } finally {
      setChecking(false);
    }
  };

  useEffect(() => {
    void check();
  }, [bridge]);

  const install = async () => {
    setInstalling(true);
    setError(false);
    setDownload(null);
    try {
      await bridge.call("update/install", undefined, (event) => setDownload(event));
    } catch {
      setStatus(null);
      setError(true);
      setInstalling(false);
    }
  };

  const available = status?.available === true;
  // Shown only when there is something the user must act on: an update to
  // install, an install in progress, or an install that failed.
  if (status?.configured === false || dismissed || !(available || installing || error)) return null;
  return (
    <aside className="update-notice" role="status" aria-live="polite">
      <div className="update-notice-copy">
        <strong>
          {installing
            ? s.t("update.installing")
            : available
              ? s.t("update.available", { version: status.version ?? "" })
              : s.t("update.failed")}
        </strong>
        {available && status.notes && <span>{status.notes}</span>}
        {installing && download && download.total > 0 && (
          <span>{Math.min(100, Math.floor((download.done / download.total) * 100))}%</span>
        )}
      </div>
      {available && (
        <button className="button primary" type="button" disabled={installing} onClick={() => void install()}>
          {s.t(installing ? "update.installing" : "update.install")}
        </button>
      )}
      {!available && error && (
        <button className="button secondary" type="button" disabled={checking || installing} onClick={() => void check()}>
          {s.t("update.check")}
        </button>
      )}
      {!installing && (
        <button className="button secondary" type="button" onClick={() => setDismissed(true)}>
          {s.t("update.dismiss")}
        </button>
      )}
    </aside>
  );
}
