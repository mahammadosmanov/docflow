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
  "No write grant covering RFI Log": "error.rfi.writeGrantMissing",
  "RFI Log workbook has no parent": "error.rfi.workbookNoFolder",
  "gecersiz sheet plani": "error.rfi.sheetPlanInvalid",
  "gecersiz coklu-sheet RFI plani": "error.rfi.multiSheetPlanInvalid",
  "gecersiz RFI satiri": "error.rfi.rowInvalid",
  "mail tablosu kaynagi gecersiz": "error.mail.tableSourceInvalid",
  "bu planda kopyalanabilir tablo yok": "error.plan.noCopyableTable",
  "copy source could not be encoded": "error.clipboard.encodeFailed",
  "path has no existing ancestor": "error.path.noAncestor",
  "watch update task failed": "error.watch.updateFailed",
  "RFI schedule date is out of range": "error.rfi.scheduleDateRange",
  "RFI system entry count overflow": "error.rfi.systemEntryLimit",
  "Follow Up workbook has no parent": "error.rfi.followUp.noParent",
  "RFI file-group row identity is invalid": "error.rfi.fileGroup.rowInvalid",
  "workbook hash is unavailable": "error.rfi.workbookUnreadable",
  "workbook metadata is unavailable": "error.rfi.workbookUnreadable",
  "workbook has no parent directory": "error.path.noParent",
  "workbook path cannot be canonicalized": "error.path.canonicalize",
  "print queue could not be saved": "error.print.queueSaveFailed",
  "print queue could not be encoded": "error.print.queueEncodeFailed",
  "print queue encoding failed": "error.print.queueEncodeFailed",
  "saved print queue is invalid": "error.print.queueInvalid",
  "saved print job is invalid": "error.print.jobInvalid",
  "path must be absolute": "error.path.notAbsolute",
  "path selection task failed": "error.path.selectionFailed",
  "path reveal task failed": "error.path.revealFailed",
  "selected path cannot be opened safely": "error.path.openUnsafe",
  "native preview task failed": "error.operation.previewFailed",
  "native undo task failed": "error.operation.undoFailed",
  "scan source could not be read": "error.scan.sourceUnreadable",
  "review directory could not be read": "error.review.directoryUnreadable",
  "RFI rows could not be encoded": "error.rfi.rowsEncodeFailed",
  "RFI file-group rows are missing": "error.rfi.fileGroup.rowsMissing",
  "RFI file-group rows are invalid": "error.rfi.fileGroup.rowsInvalid",
  "RFI file-group row state is invalid": "error.rfi.fileGroup.stateInvalid",
  "RFI file-group row is not approved": "error.rfi.fileGroup.notApproved",
  "RFI file source entry changed": "error.rfi.sourceChanged",
  "RFI file source entry could not be read": "error.rfi.sourceUnreadable",
  "diagnostics bundle is invalid or was changed": "error.support.bundleInvalid",
  "generated destination is missing": "error.path.generatedMissing",
  "invalid print queue id": "error.print.queueIdInvalid",
  "invalid print row": "error.print.rowInvalid",
  "native backend task failed": "error.operation.backendFailed",
  "native apply task failed": "error.operation.applyFailed",
  "diagnostics preview encoding failed": "error.support.previewFailed",
  "diagnostics collection task failed": "error.support.collectFailed",
  "diagnostics clipboard task failed": "error.support.clipboardFailed",
  "clipboard write failed": "error.clipboard.writeFailed",
  "created directory is unavailable": "error.path.createdDirUnavailable",
  "created directory is not empty": "error.path.createdDirNotEmpty",
  "authorized root is unavailable": "error.path.rootUnavailable",
  "authorized root cannot be canonicalized": "error.path.canonicalize",
  "Selected input has no parent folder": "error.path.noParent",
  "RFI schedule time is invalid": "error.rfi.scheduleTimeInvalid",
  "RFI folder metadata failed": "error.rfi.folderReadFailed",
  "RFI folder entry failed": "error.rfi.folderReadFailed",
  "RFI document discovery failed": "error.rfi.discoveryFailed",
  "RFI PDF could not be resolved": "error.rfi.pdfUnresolved",
  "RFI file source could not be read": "error.rfi.sourceUnreadable",
  "RFI file name is not valid Unicode": "error.rfi.fileNameInvalid",
  "PDF escaped the selected RFI root": "error.rfi.pdfOutsideRoot",
  "Gemini response is not an object": "error.gemini.invalidResponse",
  "Gemini extraction is invalid": "error.gemini.invalidResponse",
  "Gemini extraction DTO is invalid": "error.gemini.invalidResponse",
  "Gemini lookup mapping is invalid": "error.gemini.invalidResponse",
  "Gemini discipline is invalid": "error.gemini.invalidResponse",
  "Gemini model name is invalid": "error.gemini.invalidResponse",
  "review path is blocked": "error.review.pathBlocked",
  "review destination has no parent": "error.review.destinationNoParent",
  "review PDF has no parent": "error.review.pdfNoParent",
  "print file identity is unavailable": "error.print.fileIdentityUnavailable",
  "print file directory is unavailable": "error.print.fileDirectoryUnavailable",
  "print queue directory is unavailable": "error.print.queueDirectoryUnavailable",
  "saved print job is not resumable": "error.print.jobNotResumable",
  "saved print job set changed": "error.print.jobSetChanged",
  "selected path cannot be inspected": "error.path.inspectFailed",
  "response encoding failed": "error.backend.encodeFailed",
  "path selection response encoding failed": "error.backend.encodeFailed",
  "preset value is invalid": "error.preset.valueInvalid",
  "scan destination changed during Preview revision": "error.scan.destinationChanged",
  "recoverable trash is unavailable for batch scans": "error.scan.trashUnavailable",
  "clipboard table exceeded the size limit": "error.clipboard.tooLarge",
  "scan source changed during Preview": "error.scan.sourceChanged",
  "Gemini returned an invalid structured response": "error.gemini.invalidResponse",
  "Gemini extraction values are out of range": "error.gemini.outOfRange",
  "Gemini API key is missing": "error.gemini.keyMissing",
  "Gemini request failed": "error.gemini.requestFailed",
  "Gemini is unavailable": "error.gemini.unavailable",
  "No supported Gemini model was available": "error.gemini.noModel",
  "printer is unavailable": "error.print.printerUnavailable",
  "copies must be between 1 and 99": "error.print.copiesRange",
  "saved print queue has no jobs": "error.print.queueEmpty",
  "print plan has no jobs": "error.print.planNoJobs",
  "selected path is invalid": "error.path.selectedInvalid",
  "selected path could not be opened": "error.path.openFailed",
  "dropped file type is not accepted": "error.path.droppedTypeInvalid",
  "dropped path must be a folder": "error.path.droppedNotFolder",
  "dropped path must be a file": "error.path.droppedNotFile",
  "dropped path does not exist": "error.path.droppedMissing",
  "history limit must be between 1 and 100": "error.history.limitRange",
  "RFI system root is not a directory": "error.rfiSystem.rootNotFolder",
  "RFI system root could not be resolved": "error.rfiSystem.rootUnresolved",
  "RFI system folder could not be read": "error.rfiSystem.folderUnreadable",
  "kopyalanacak sheet secilmelidir": "error.rfi.copySheetRequired",
  "en az bir RFI satiri gerekli": "error.rfi.rowRequired",
  "tek planda en fazla 500 fiziksel satir olabilir": "error.rfi.rowLimit",
  "RFI sheet gruplari eklenemez veya silinemez": "error.rfi.sheetGroupsLocked",
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

/** Name of the required parameter a preview rejected, or null when the error is about something else. */
export function missingParamName(error: unknown): string | null {
  const raw = error instanceof Error ? error.message : String(error);
  return (
    raw.match(/^(?:Error:\s*)?error\.param\.required\.([A-Za-z0-9_]+)$/)?.[1] ??
    null
  );
}
