import {
  User,
} from "lucide-react";
import {
  type Strings,
} from "./i18n";
import {
  type AppPrefs,
  type OperationDef,
  type PathSelection,
  type Plan,
} from "./types";

export type Route =
  | { name: "home" }
  | {
      name: "operation";
      operation: OperationDef;
      initialValues?: Record<string, unknown>;
      initialPlan?: Plan;
    }
  | { name: "review"; destination: PathSelection }
  | { name: "recovery" }
  | { name: "queue" }
  | { name: "history" }
  | { name: "admin" }
  | { name: "settings"; back: "home" | "login" | "pending" };

export const usernamePattern = /^[a-zA-Z0-9._-]{3,24}$/;

export function isPathSelection(value: unknown): value is PathSelection {
  return Boolean(
    value &&
    typeof value === "object" &&
    "selectionId" in value &&
    typeof value.selectionId === "string" &&
    "displayPath" in value &&
    typeof value.displayPath === "string",
  );
}

export function parseRfiInputs(value: unknown): string[] {
  if (typeof value !== "string") return [];
  return [...new Set(value.split(/[\s,;]+/).map((item) => item.trim()).filter(Boolean))];
}

const PATH_ERROR_KEYS: Record<string, string> = {
  path_traversal: "error.path.traversal",
  path_escape: "error.path.escape",
  path_access_denied: "error.path.accessDenied",
  path_grant_not_found_or_expired: "error.path.grantExpired",
  path_grant_scope_mismatch: "error.path.grantMismatch",
};

/** Backend messages that are shown as plain English text today, keyed by their exact text. */
export const BACKEND_MESSAGE_KEYS: Record<string, string> = {
  "print file is unavailable": "error.print.fileUnavailable",
  "unknown operation ID": "error.operation.unknown",
  "invalid saved print queue": "error.print.queueInvalid",
  "print plan has no rows": "error.print.planEmpty",
  "review destination does not exist": "error.review.destinationMissing",
  "source path does not exist": "error.path.sourceMissing",
  "selected path does not exist": "error.path.selectedMissing",
  "saved print job no longer exists": "error.print.jobMissing",
  "recoverable trash does not exist": "error.trash.missing",
  "path cannot be canonicalized": "error.path.canonicalize",
  "print file timestamp is unavailable": "error.print.timestampMissing",
  "review source must be a PDF": "error.review.sourceNotPdf",
  "review source does not exist": "error.review.sourceMissing",
  "review directory does not exist": "error.review.directoryMissing",
  "review source name is invalid": "error.review.sourceNameInvalid",
  "review source cannot be resolved": "error.review.sourceUnresolved",
  "operation cannot be watched": "error.watch.unsupported",
  "diagnostics bundle encoding failed": "error.support.encodingFailed",
  "granted root identity is unavailable": "error.path.rootUnavailable",
  "invalid print row identity": "error.print.rowInvalid",
};

export function humanError(error: unknown, s: Strings): string {
  const code =
    typeof error === "object" && error !== null && "code" in error
      ? String(error.code)
      : "";
  const pathKey = PATH_ERROR_KEYS[code];
  if (pathKey) return s.t(pathKey);
  const raw = error instanceof Error ? error.message : String(error);
  const backendKey = BACKEND_MESSAGE_KEYS[raw.replace(/^Error:\s*/, "")];
  if (backendKey) return s.t(backendKey);
  const localizedKey = raw.match(
    /^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+)$/,
  )?.[1];
  if (localizedKey) {
    const localized = s.t(localizedKey);
    if (localized !== localizedKey) return localized;
  }
  const detailedMatch = raw.match(
    /^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+):\s*(.+)$/,
  );
  if (detailedMatch) {
    const key = detailedMatch[1];
    const details = detailedMatch[2];
    const localized = s.t(key);
    if (localized !== key) {
      return `${localized} (${details})`;
    }
  }
  if (/error\.preset\.nameInvalid|preset[_ .-]?name[_ .-]?invalid/i.test(raw))
    return s.t("error.preset.nameInvalid");
  if (
    /error\.preset\.duplicate|duplicate[_ .-]?preset|preset.*already exists/i.test(
      raw,
    )
  )
    return s.t("error.preset.duplicate");
  if (/error\.preset\.limit|preset[_ .-]?(?:capacity|limit)/i.test(raw))
    return s.t("error.preset.limit");
  if (
    /error\.preset\.stale|preset[_ .-]?(?:missing|stale|not found)/i.test(raw)
  )
    return s.t("error.preset.stale");
  if (/error\.preset\.write|preset.*(?:write|persist|save).*fail/i.test(raw))
    return s.t("error.preset.write");
  if (/^(?:Error:\s*)?error\.rfi\.workbookInvalid$/.test(raw))
    return s.t("error.rfi.workbookInvalid");
  if (/^(?:Error:\s*)?error\.print\.queueSourceReselectionRequired$/.test(raw))
    return s.t("error.print.queueSourceReselectionRequired");
  if (/email_required/i.test(raw)) return s.t("auth.emailSignInHelp");
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

export function isDuplicatePresetError(error: unknown): boolean {
  const raw = error instanceof Error ? error.message : String(error);
  return /error\.preset\.duplicate|duplicate[_ .-]?preset|preset.*already exists/i.test(
    raw,
  );
}

export function applyAppearance(prefs: AppPrefs): void {
  const root = document.documentElement;
  root.dataset.theme = prefs.theme;
  root.dataset.font = prefs.font;
  root.dataset.compact = String(prefs.compact);
  root.style.fontSize = `${prefs.fontScale * 16}px`;
}

export function formatDate(value: string, locale: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}
