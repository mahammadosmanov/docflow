export type ParamKind =
  | "folder"
  | "file"
  | "files"
  | "text"
  | "bool"
  | "choice"
  | "int"
  | "longText"
  | "secret"
  | "sheetCounts";

export interface ParamDef {
  name: string;
  kind: ParamKind;
  labelKey: string;
  default: unknown;
  required: boolean;
  choices: string[];
  choiceLabels: Record<string, string>;
  visibleWhen: Record<string, unknown>;
  helpKey: string | null;
  remember?: boolean;
}

export interface OperationDef {
  id: string;
  titleKey: string;
  descriptionKey: string;
  schema: ParamDef[];
  supportsPresets: boolean;
}

export interface OperationPreset {
  id: string;
  name: string;
  values: Record<string, unknown>;
}

export interface Step {
  kind:
    | "mkdir"
    | "move"
    | "copy"
    | "rename"
    | "write"
    | "workbook_update"
    | "restore_backup"
    | "print"
    | "pdf_first_page_replace"
    | "pdf_page_rotate"
    | "pdf_pages_extract"
    | "replica_publish"
    | "cleanup_temp"
    | "outlook_reply_draft";
  src: string | null;
  dst: string | null;
  noteKey: string | null;
  meta: Record<string, unknown>;
}

export interface Warning {
  key: string;
  subject: string;
}

export interface Plan {
  planId: string;
  opId: string;
  steps: Step[];
  warnings: Warning[];
  meta: Record<string, unknown>;
}

export interface EditorMetadata extends Record<string, unknown> {
  reorderable?: boolean;
}

export interface OperationResult {
  applied: number;
  failed: number;
  journalId: string;
  errors: Warning[];
  meta?: Record<string, unknown>;
  cloudSynced?: boolean;
}

export interface ReplicaAutomationStatus {
  enabled: boolean;
  configured: boolean;
  status:
    | "unconfigured"
    | "disabled"
    | "idle"
    | "running"
    | "waitingForSignIn"
    | "deferred"
    | "paused";
  lastSuccessUnix: number | null;
  lastErrorCode: string | null;
}

export interface PrintQueueSummary {
  queueId: string;
  status: string;
  createdAt: string;
  updatedAt: string;
  counts: Record<string, number>;
  resumable: number;
  fileCount: number;
}

export interface AppUser {
  id: string;
  email: string;
  username: string | null;
  display: string;
  allowed: boolean;
  isAdmin: boolean;
  createdAt: string;
}

export interface AuthState {
  signedIn: boolean;
  allowed: boolean;
  isAdmin: boolean;
  session: {
    userId: string;
    email: string;
    expiresAt: number;
  } | null;
  user: AppUser | null;
}

export type ThemePreference = "system" | "light" | "dark";
export type FontPreference = "system" | "sans" | "serif" | "mono";

export interface AppPrefs {
  locale: "tr" | "en" | "ru" | "az" | null;
  theme: ThemePreference;
  font: FontPreference;
  fontScale: number;
  compact: boolean;
  startWithWindows: boolean;
  replicaSyncEnabled: boolean;
}

export interface Bootstrap {
  protocol: number;
  version: string;
  auth: AuthState;
  prefs: AppPrefs;
  operations: OperationDef[];
}

export interface ActivityLogExport {
  fileName: string;
  mediaType: string;
  content: string;
}

export interface SupportBundlePreview {
  fileName: string;
  mediaType: "application/json";
  byteLength: number;
  content: string;
}

export interface ProgressEvent {
  phase: string;
  done: number;
  total: number;
  subject?: string;
}

export type ProgressEventHandler = (event: ProgressEvent) => void;

export interface PathSelection {
  selectionId: string;
  displayPath: string;
  name: string;
}

export interface RfiPrintCandidate {
  requested: string;
  rfiCode: string;
  documentType: "firstPage" | "sketch";
  displayPath: string;
  selection: PathSelection;
  selected: boolean;
}

export interface RfiPrintResolution {
  requested: string;
  rfiCodes: string[];
  missingFirstPage: boolean;
  missingSketch: boolean;
  ambiguous: boolean;
}

export interface RfiPrintFindResult {
  candidates: RfiPrintCandidate[];
  resolutions: RfiPrintResolution[];
  visited: number;
  truncated: boolean;
}

export interface ReviewItem {
  selectionId: string;
  path: string;
  name: string;
  code: string;
  confidence: number;
  targetExists?: boolean;
}

export type ReviewAction = "keep" | "put" | "delete";

export interface ReviewDecision {
  itemId: string;
  action: Exclude<ReviewAction, "keep">;
  code: string;
}

export interface HistoryRow {
  when: string;
  opId: string;
  moves: number;
  failed: number;
  journalId: string;
  steps: Step[] | null;
  cloudId: string | null;
  undone: boolean;
  undoAvailable: boolean;
  fromCloud: boolean;
  userEmail: string;
}

export interface HistoryPayload {
  online: boolean;
  rows: HistoryRow[];
}

export interface RecoveryEntry {
  journalId: string;
  disposition: "no_effect" | "outcome_unknown";
  restoreOriginalAvailable?: boolean;
}

export interface RecoveryPayload {
  journals: RecoveryEntry[];
  printQueue: PrintQueueSummary | null;
}

export interface WatchItem {
  opId: string;
  status: "preparing" | "ready" | "stale" | "failed";
  planId: string | null;
  steps: number;
  warnings: number;
  errorCode: string | null;
  preparedAt: number;
}

export interface WatchStatus {
  operations: string[];
  enabled: string[];
  configured: string[];
  pollSeconds: number;
  paused: boolean;
  items: WatchItem[];
}
