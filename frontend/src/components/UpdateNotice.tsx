import { useEffect, useState } from "react";
import type { Strings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { ProgressEvent, UpdateCheckResult } from "../types";

export function UpdateNotice({ bridge, s }: { bridge: Bridge; s: Strings }) {
  const [status, setStatus] = useState<UpdateCheckResult | null>(null);
  const [checking, setChecking] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState(false);
  const [download, setDownload] = useState<ProgressEvent | null>(null);

  const check = async () => {
    setChecking(true);
    setError(false);
    try {
      setStatus(await bridge.call("update/check"));
    } catch {
      setStatus(null);
      setError(true);
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

  if (status?.configured === false) return null;
  const available = status?.available === true;
  return (
    <aside className="update-notice" role="status" aria-live="polite">
      <div className="update-notice-copy">
        <strong>
          {installing
            ? s.t("update.installing")
            : checking
              ? s.t("update.checking")
              : available
                ? s.t("update.available", { version: status.version ?? "" })
                : error
                  ? s.t("update.failed")
                  : s.t("update.current", { version: status?.currentVersion ?? "" })}
        </strong>
        {error && status && <span>{s.t("update.failed")}</span>}
        {available && status.notes && <span>{status.notes}</span>}
        {installing && download && download.total > 0 && (
          <span>{Math.min(100, Math.floor((download.done / download.total) * 100))}%</span>
        )}
      </div>
      {available ? (
        <button className="button primary" type="button" disabled={installing} onClick={() => void install()}>
          {s.t(installing ? "update.installing" : "update.install")}
        </button>
      ) : (
        <button className="button secondary" type="button" disabled={checking || installing} onClick={() => void check()}>
          {s.t("update.check")}
        </button>
      )}
    </aside>
  );
}
