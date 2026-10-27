import { useState } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type SupportBundlePreview } from "../types";

/** Support diagnostics: preview the bundle, copy it, save the activity log, or download the bundle. */
export function useSupportDiagnostics({
  bridge,
  s,
  setError,
}: {
  bridge: Bridge;
  s: Strings;
  setError: (message: string) => void;
}) {
  const [diagnostics, setDiagnostics] = useState<SupportBundlePreview | null>(
    null,
  );
  const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [diagnosticsStatus, setDiagnosticsStatus] = useState("");

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
const saveActivityLog = async () => {
  setError("");
  setDiagnosticsStatus("");
  setDiagnosticsBusy(true);
  try {
    const log = await bridge.call("support/activityLog");
    const blob = new Blob([log.content], { type: log.mediaType });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = log.fileName;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    setDiagnosticsStatus(s.t("prefs.support.activityLogSaved"));
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

  return {
    diagnostics,
    diagnosticsBusy,
    diagnosticsStatus,
    previewDiagnostics,
    copyDiagnostics,
    saveActivityLog,
    downloadDiagnostics,
  };
}
