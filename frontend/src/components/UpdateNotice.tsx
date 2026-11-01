import { useEffect, useState } from "react";
import type { Strings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { ProgressEvent, UpdateCheckResult } from "../types";

// Floating banner by default; `inline` renders the Settings row that always shows status.
export function UpdateNotice({ bridge, s, inline = false }: { bridge: Bridge; s: Strings; inline?: boolean }) {
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
      setError(inline);
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
  if (status?.configured === false) return null;
  // Floating banner shows only when the user must act: update, install in progress, or failed install.
  if (!inline && (dismissed || !(available || installing || error))) return null;

  const headline = installing
    ? s.t("update.installing")
    : available
      ? s.t("update.available", { version: status.version ?? "" })
      : error
        ? s.t("update.failed")
        : checking
          ? s.t("update.checking")
          : s.t("update.current", { version: status?.currentVersion ?? "" });

  return (
    <aside className={inline ? "update-notice update-notice-inline" : "update-notice"} role="status" aria-live="polite">
      <div className="update-notice-copy">
        <strong>{headline}</strong>
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
      {!available && (inline || error) && (
        <button className="button secondary" type="button" disabled={checking || installing} onClick={() => void check()}>
          {s.t("update.check")}
        </button>
      )}
      {!inline && !installing && (
        <button className="button secondary" type="button" onClick={() => setDismissed(true)}>
          {s.t("update.dismiss")}
        </button>
      )}
    </aside>
  );
}
