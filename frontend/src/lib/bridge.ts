import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  CommandArgs,
  CommandParams,
  CommandResult,
  FrontendMethod,
} from "./commands";

import type {
  AppPrefs,
  AppUser,
  AuthState,
  Bootstrap,
  HistoryPayload,
  OperationDef,
  ParamDef,
  OperationResult,
  PathSelection,
  Plan,
  ProgressEvent,
  RecoveryPayload,
  ReplicaAutomationStatus,
  ReviewItem,
  SupportBundlePreview,
} from "../types";

type ProgressHandler = (event: ProgressEvent) => void;

export interface Bridge {
  start(): Promise<void>;
  call<Method extends FrontendMethod>(
    method: Method,
    ...args: CommandArgs<Method>
  ): Promise<CommandResult<Method>>;
  stop(): Promise<void>;
}

export class BridgeError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "BridgeError";
  }
}

function asError(error: unknown): Error {
  if (error instanceof Error) return error;
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    const code =
      "code" in error && typeof error.code === "string"
        ? error.code
        : "backend_request_failed";
    return new BridgeError(code, error.message);
  }
  return new Error(String(error));
}

export class TauriBridge implements Bridge {
  async start(): Promise<void> {
    // Native commands initialize lazily behind the Rust boundary.
  }

  async call<Method extends FrontendMethod>(
    method: Method,
    ...args: CommandArgs<Method>
  ): Promise<CommandResult<Method>> {
    const [params, onProgress] = args as [
      CommandParams<Method> | undefined,
      ProgressHandler | undefined,
    ];
    const progress = new Channel<ProgressEvent>();
    progress.onmessage = (event) => onProgress?.(event);
    try {
      return await invoke<CommandResult<Method>>("frontend_call", {
        request: { method, params: params ?? {} },
        onProgress: progress,
      });
    } catch (error) {
      throw asError(error);
    }
  }

  async stop(): Promise<void> {
    // The final React surface has no process lifecycle authority.
  }
}

const demoOperation: OperationDef = {
  id: "rfi.process_scans",
  titleKey: "op.rfi.process_scans.title",
  descriptionKey: "op.rfi.process_scans.desc",
  supportsPresets: false,
  schema: [
    {
      name: "source",
      kind: "folder",
      labelKey: "param.source",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.source.help",
    },
    {
      name: "dest",
      kind: "folder",
      labelKey: "param.dest",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.dest.help",
    },
    {
      name: "keep_original",
      kind: "bool",
      labelKey: "param.keep_original",
      default: false,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: null,
    },
  ],
};

const demoFileGroupOperation: OperationDef = {
  id: "rfi.group_files",
  titleKey: "op.rfi.group_files.title",
  descriptionKey: "op.rfi.group_files.desc",
  supportsPresets: false,
  schema: [
    {
      name: "source",
      kind: "folder",
      labelKey: "param.rfi.fileGroup.source",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.fileGroup.source.help",
    },
    {
      name: "destination",
      kind: "folder",
      labelKey: "param.rfi.fileGroup.destination",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.fileGroup.destination.help",
    },
  ],
};

const demoRfiOperation: OperationDef = {
  id: "rfi.log_add",
  titleKey: "op.rfi.log_add.title",
  descriptionKey: "op.rfi.log_add.desc",
  supportsPresets: false,
  schema: [
    {
      name: "workbook",
      kind: "file",
      labelKey: "param.rfi.workbook",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.workbook.help",
    },
    {
      name: "email_text",
      kind: "longText",
      labelKey: "param.rfi.emailText",
      default: "",
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.emailText.help",
    },
    {
      name: "email_provider",
      kind: "choice",
      labelKey: "param.rfi.emailProvider",
      default: "offline",
      required: true,
      choices: ["offline", "gemini"],
      choiceLabels: {
        offline: "choice.rfi.emailProvider.offline",
        gemini: "choice.rfi.emailProvider.gemini",
      },
      visibleWhen: {},
      helpKey: "param.rfi.emailProvider.help",
    },
    {
      name: "gemini_api_key",
      kind: "secret",
      labelKey: "param.rfi.geminiApiKey",
      default: "",
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: { email_provider: "gemini" },
      helpKey: "param.rfi.geminiApiKey.help",
    },
    {
      name: "sheet_counts",
      kind: "sheetCounts",
      labelKey: "param.rfi.sheetCounts",
      default: { CIV: 3, MEC: 0, ELE: 0, PRC: 0 },
      required: true,
      choices: ["CIV", "MEC", "ELE", "PRC"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.sheetCounts.help",
    },
  ],
};

const demoOutlookReplyOperation: OperationDef = {
  id: "rfi.outlook_reply_draft",
  titleKey: "op.rfi.outlook_reply_draft.title",
  descriptionKey: "op.rfi.outlook_reply_draft.desc",
  supportsPresets: false,
  schema: [
    {
      name: "reply_text",
      kind: "longText",
      labelKey: "param.rfi.outlookReplyDraft.replyText",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.outlookReplyDraft.replyText.help",
    },
  ],
};

const demoRfiFollowUpOperation: OperationDef = {
  id: "rfi.follow_up_sync",
  titleKey: "op.rfi.follow_up_sync.title",
  descriptionKey: "op.rfi.follow_up_sync.desc",
  supportsPresets: false,
  schema: [
    {
      name: "rfi_log_workbook",
      kind: "file",
      labelKey: "param.rfi.followUp.rfiLogWorkbook",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.followUp.rfiLogWorkbook.help",
    },
    {
      name: "follow_up_workbook",
      kind: "file",
      labelKey: "param.rfi.followUp.followUpWorkbook",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.followUp.followUpWorkbook.help",
    },
  ],
};

const demoRfiSystemSyncOperation: OperationDef = {
  id: "rfi.system_sync",
  titleKey: "op.rfi.system_sync.title",
  descriptionKey: "op.rfi.system_sync.desc",
  supportsPresets: false,
  schema: [
    {
      name: "rfi_system_root",
      kind: "folder",
      labelKey: "param.rfi.systemSync.rfiSystemRoot",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.systemSync.rfiSystemRoot.help",
    },
    {
      name: "rfi_log_workbook",
      kind: "file",
      labelKey: "param.rfi.systemSync.rfiLogWorkbook",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.systemSync.rfiLogWorkbook.help",
    },
    {
      name: "follow_up_workbook",
      kind: "file",
      labelKey: "param.rfi.systemSync.followUpWorkbook",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.systemSync.followUpWorkbook.help",
    },
    {
      name: "force_rescan",
      kind: "bool",
      labelKey: "param.scanIndex.forceRescan",
      default: false,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.scanIndex.forceRescan.help",
      remember: false,
    },
  ],
};

const demoRfiCompleteStatusOperation: OperationDef = {
  id: "rfi.complete_status_sync",
  titleKey: "op.rfi.complete_status_sync.title",
  descriptionKey: "op.rfi.complete_status_sync.desc",
  supportsPresets: false,
  schema: [
    {
      name: "rfi_system_root",
      kind: "folder",
      labelKey: "param.rfi.completeStatus.rfiSystemRoot",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.completeStatus.rfiSystemRoot.help",
    },
    {
      name: "rfi_log_workbook",
      kind: "file",
      labelKey: "param.rfi.completeStatus.rfiLogWorkbook",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.completeStatus.rfiLogWorkbook.help",
    },
  ],
};

const demoReplicaPublishOperation: OperationDef = {
  id: "files.replica_publish",
  titleKey: "op.files.replica_publish.title",
  descriptionKey: "op.files.replica_publish.desc",
  supportsPresets: false,
  schema: [
    ...(["a", "b", "c"] as const).map((suffix) => ({
      name: `root_${suffix}`,
      kind: "folder" as const,
      labelKey: `param.replica.root${suffix.toUpperCase()}`,
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: `param.replica.root${suffix.toUpperCase()}.help`,
    })),
    {
      name: "source_root",
      kind: "choice",
      labelKey: "param.replica.sourceRoot",
      default: "a",
      required: true,
      choices: ["a", "b", "c"],
      choiceLabels: {
        a: "choice.replica.source.a",
        b: "choice.replica.source.b",
        c: "choice.replica.source.c",
      },
      visibleWhen: {},
      helpKey: "param.replica.sourceRoot.help",
    },
    {
      name: "backup_root",
      kind: "folder",
      labelKey: "param.replica.backupRoot",
      default: null,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.backupRoot.help",
    },
    {
      name: "root_files",
      kind: "text",
      labelKey: "param.replica.rootFiles",
      default: "",
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.rootFiles.help",
    },
    {
      name: "unattended",
      kind: "bool",
      labelKey: "param.replica.unattended",
      default: false,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.unattended.help",
    },
    {
      name: "interval_minutes",
      kind: "int",
      labelKey: "param.replica.intervalMinutes",
      default: 15,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.intervalMinutes.help",
    },
    {
      name: "settle_seconds",
      kind: "int",
      labelKey: "param.replica.settleSeconds",
      default: 30,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.settleSeconds.help",
    },
    {
      name: "max_changed_files",
      kind: "int",
      labelKey: "param.replica.maxChangedFiles",
      default: 250,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.maxChangedFiles.help",
    },
    {
      name: "max_changed_mib",
      kind: "int",
      labelKey: "param.replica.maxChangedMib",
      default: 2048,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.replica.maxChangedMib.help",
    },
  ],
};

const demoPrintOperation: OperationDef = {
  id: "files.batch_print",
  titleKey: "op.files.batch_print.title",
  descriptionKey: "op.files.batch_print.desc",
  supportsPresets: true,
  schema: [
    {
      name: "files",
      kind: "files",
      labelKey: "param.print.files",
      default: [],
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.files.help",
      remember: false,
    },
    {
      name: "printer",
      kind: "choice",
      labelKey: "param.print.printer",
      default: "Office Printer",
      required: true,
      choices: ["Office Printer"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.printer.help",
    },
    {
      name: "copies",
      kind: "int",
      labelKey: "param.print.copies",
      default: 1,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.copies.help",
    },
    {
      name: "paper",
      kind: "choice",
      labelKey: "param.print.paper",
      default: "A4",
      required: true,
      choices: ["auto", "A4", "A3", "A5", "Letter", "Legal"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.paper.help",
    },
    {
      name: "orientation",
      kind: "choice",
      labelKey: "param.print.orientation",
      default: "auto",
      required: true,
      choices: ["auto", "portrait", "landscape"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: null,
    },
    {
      name: "scaling",
      kind: "choice",
      labelKey: "param.print.scaling",
      default: "fit",
      required: true,
      choices: ["fit", "actual", "shrink"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: null,
    },
    {
      name: "color",
      kind: "choice",
      labelKey: "param.print.color",
      default: "printer",
      required: true,
      choices: ["printer", "color", "grayscale"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: null,
    },
    {
      name: "duplex",
      kind: "choice",
      labelKey: "param.print.duplex",
      default: "printer",
      required: true,
      choices: ["printer", "simplex", "long", "short"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: null,
    },
    {
      name: "collate",
      kind: "bool",
      labelKey: "param.print.collate",
      default: true,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.collate.help",
    },
    {
      name: "retries",
      kind: "int",
      labelKey: "param.print.retries",
      default: 0,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.retries.help",
    },
    {
      name: "on_error",
      kind: "choice",
      labelKey: "param.print.onError",
      default: "pause",
      required: true,
      choices: ["pause", "skip"],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.onError.help",
    },
    {
      name: "timeout_minutes",
      kind: "int",
      labelKey: "param.print.timeout",
      default: 30,
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.print.timeout.help",
    },
  ],
};

const demoSignedPagesOperation: OperationDef = {
  id: "rfi.stage_signed_first_pages",
  titleKey: "op.rfi.stage_signed_first_pages.title",
  descriptionKey: "op.rfi.stage_signed_first_pages.desc",
  supportsPresets: false,
  schema: [
    {
      name: "signed_scans",
      kind: "folder",
      labelKey: "param.rfi.signedScans",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.signedScans.help",
    },
    {
      name: "archive_root",
      kind: "folder",
      labelKey: "param.rfi.archiveRoot",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.archiveRoot.help",
    },
    {
      name: "review_root",
      kind: "folder",
      labelKey: "param.rfi.reviewRoot",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.reviewRoot.help",
    },
    {
      name: "scan_subfolders",
      kind: "bool",
      labelKey: "param.rfi.scanSubfolders",
      default: true,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.scanSubfolders.help",
    },
  ],
};

const demoPdfOrientationOperation: OperationDef = {
  id: "rfi.normalize_pdf_orientation",
  titleKey: "op.rfi.normalize_pdf_orientation.title",
  descriptionKey: "op.rfi.normalize_pdf_orientation.desc",
  supportsPresets: false,
  schema: [
    {
      name: "rfi_system_root",
      kind: "folder",
      labelKey: "param.rfi.orientation.rfiSystemRoot",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.orientation.rfiSystemRoot.help",
    },
    {
      name: "review_root",
      kind: "folder",
      labelKey: "param.rfi.orientation.reviewRoot",
      default: "",
      required: true,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.orientation.reviewRoot.help",
    },
    {
      name: "force_rescan",
      kind: "bool",
      labelKey: "param.scanIndex.forceRescan",
      default: false,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.scanIndex.forceRescan.help",
      remember: false,
    },
  ],
};

const signedOut: AuthState = {
  signedIn: false,
  allowed: false,
  isAdmin: false,
  session: null,
  user: null,
};

const demoAdmin: AppUser = {
  id: "demo-user",
  email: "demo@docflow.local",
  username: "demo",
  display: "demo",
  allowed: true,
  isAdmin: true,
  createdAt: new Date().toISOString(),
};

const defaultPrefs: AppPrefs = {
  locale: "tr",
  theme: "system",
  font: "system",
  fontScale: 1,
  compact: false,
  startWithWindows: false,
  replicaSyncEnabled: false,
};

function mockPath(value: unknown, fallback: string): string {
  return value &&
    typeof value === "object" &&
    "displayPath" in value &&
    typeof value.displayPath === "string"
    ? value.displayPath
    : fallback;
}

/** Browser-only bridge used by component tests and visual development. */
export class MockBridge implements Bridge {
  private auth: AuthState = signedOut;
  private prefs: AppPrefs = { ...defaultPrefs };
  private params: Record<string, unknown> = {};
  private printPlan: Plan | null = null;
  private fileGroupPlan: Plan | null = null;
  private completeStatusPlan: Plan | null = null;

  async start(): Promise<void> {}
  async stop(): Promise<void> {}

  async call<Method extends FrontendMethod>(
    method: Method,
    ...args: CommandArgs<Method>
  ): Promise<CommandResult<Method>> {
    const [rawParams, onProgress] = args as [
      CommandParams<Method> | undefined,
      ProgressHandler | undefined,
    ];
    const params = (rawParams ?? {}) as Record<string, unknown>;
    await new Promise((resolve) => window.setTimeout(resolve, 30));
    let result: unknown;
    switch (method) {
      case "app/bootstrap":
        result = {
          protocol: 1,
          version: `${__DOCFLOW_VERSION__}-demo`,
          auth: this.auth,
          prefs: this.prefs,
          operations: [
            demoOperation,
            demoFileGroupOperation,
            demoRfiOperation,
            demoOutlookReplyOperation,
            demoRfiFollowUpOperation,
            demoPrintOperation,
            demoSignedPagesOperation,
            demoPdfOrientationOperation,
            demoRfiSystemSyncOperation,
            demoRfiCompleteStatusOperation,
            demoReplicaPublishOperation,
          ],
        } satisfies Bootstrap;
        break;
      case "auth/signIn":
        this.auth = {
          signedIn: true,
          allowed: true,
          isAdmin: true,
          session: {
            userId: demoAdmin.id,
            email: demoAdmin.email,
            expiresAt: Date.now() / 1000 + 3600,
          },
          user: demoAdmin,
        };
        result = this.auth;
        break;
      case "auth/signUp":
        result = { ok: true };
        break;
      case "auth/signOut":
        this.auth = signedOut;
        result = this.auth;
        break;
      case "auth/loadMe":
      case "auth/setUsername":
        result = this.auth;
        break;
      case "auth/usernameAvailable":
        result = true;
        break;
      case "auth/listUsers":
        result = [
          demoAdmin,
          {
            ...demoAdmin,
            id: "demo-user-2",
            email: "operator@docflow.local",
            username: "operator",
            display: "operator",
            isAdmin: false,
          },
        ] satisfies AppUser[];
        break;
      case "auth/setFlags":
        result = { ok: true };
        break;
      case "settings/update":
        this.prefs = { ...this.prefs, ...params } as AppPrefs;
        result = this.prefs;
        break;
      case "settings/reset":
        this.prefs = { ...defaultPrefs };
        result = this.prefs;
        break;
      case "settings/replicaStatus":
        result = {
          enabled: this.prefs.replicaSyncEnabled,
          configured: false,
          status: "idle",
          lastSuccessUnix: null,
          lastErrorCode: null,
        } satisfies ReplicaAutomationStatus;
        break;
      case "settings/params":
        result = this.params;
        break;
      case "settings/saveParams":
        this.params = (params.values as Record<string, unknown>) ?? {};
        result = { ok: true };
        break;
      case "support/diagnostics": {
        const content = `${JSON.stringify(
          {
            schemaVersion: 1,
            docflow: { version: __DOCFLOW_VERSION__ },
            platform: { arch: "demo", os: "browser-preview" },
            components: {
              ocr: {
                engine: "tesseract",
                languageDataAvailable: false,
                status: "missing",
              },
              sidecar: { required: false, status: "retired" },
              printer: {
                inventoryStatus: "available",
                printerCount: 1,
                defaultPrinterAvailable: true,
                latestQueue: null,
              },
            },
            recentErrorCodes: [],
            privacy: { delivery: "local-only", redaction: "recursive" },
          },
          null,
          2,
        )}\n`;
        result = {
          fileName: "docflow-support-diagnostics-v1.json",
          mediaType: "application/json",
          byteLength: new TextEncoder().encode(content).byteLength,
          content,
        } satisfies SupportBundlePreview;
        break;
      }
      case "support/copyDiagnostics":
        result = { ok: true };
        break;
      case "engine/preview": {
        onProgress?.({ phase: "scan", done: 1, total: 1 });
        const values = params.values as Record<string, unknown>;
        if (params.opId === "rfi.complete_status_sync") {
          const code = "AZE-SUN-INT-QMS-RFI-MEC-0700";
          const workbook = mockPath(
            values.rfi_log_workbook,
            "/Users/demo/Documents/RFI Log.xlsm",
          );
          const row = {
            _rowId: "complete-row-1",
            enabled: true,
            selectable: true,
            discipline: "MEC",
            rfiNo: "MEC-0700",
            canonicalRfi: code,
            pdfPath: `MEC/${code}/RFI No. ${code}.pdf`,
            pdfCount: 1,
            pdfPaths: `MEC/${code}/RFI No. ${code}.pdf`,
            currentStatus: "Ongoing",
            proposedStatus: "Completed",
            action: "Set matching Inspection Status cell(s) to Completed",
            disposition: "rfi.completeStatus.disposition.ready",
            reasonKey: "Exact completed RFI PDF and safe RFI Log mapping found",
          };
          const plan = {
            planId: "demo-complete-status-plan",
            opId: "rfi.complete_status_sync",
            warnings: [],
            meta: {
              workerCount: 4,
              directoriesScanned: 42,
              entriesScanned: 500,
              exactPdfCount: 1,
              editor: {
                kind: "recordTable",
                layout: "compact",
                titleKey: "rfi.completeStatus.editor.title",
                rowCountKey: "rfi.completeStatus.editor.rowCount",
                groupField: "discipline",
                selectionGroupFields: [],
                selectionEnabled: true,
                bulkEnabled: true,
                detailsEnabled: true,
                detailsDefaultVisible: false,
                duplicateEnabled: false,
                requiredFields: ["enabled"],
                columns: [
                  { key: "enabled", labelKey: "rfi.completeStatus.column.enabled", kind: "bool" },
                  { key: "discipline", labelKey: "rfi.completeStatus.column.discipline", kind: "readonly" },
                  { key: "rfiNo", labelKey: "rfi.completeStatus.column.rfiNo", kind: "readonly" },
                  { key: "pdfPath", labelKey: "rfi.completeStatus.column.pdfPath", kind: "readonly" },
                  { key: "currentStatus", labelKey: "rfi.completeStatus.column.currentStatus", kind: "readonly" },
                  { key: "proposedStatus", labelKey: "rfi.completeStatus.column.proposedStatus", kind: "readonly" },
                  { key: "disposition", labelKey: "rfi.completeStatus.column.disposition", kind: "readonly", display: "localizedKey" },
                ],
                rows: [row],
              },
            },
            steps: [
              {
                kind: "workbook_update" as const,
                src: workbook,
                dst: `${workbook}.completion-backup`,
                noteKey: "note.rfi.completeStatus.rfiLog",
                meta: {
                  syncStep: {
                    workbookKind: "rfiLogStatus",
                    updates: {
                      MEC: {
                        "10:16": {
                          rowId: row._rowId,
                          expectedValue: "Ongoing",
                          replacementValue: "Completed",
                          row: 10,
                          col: 16,
                        },
                      },
                    },
                  },
                },
              },
            ],
          } satisfies Plan;
          this.completeStatusPlan = plan;
          result = plan;
          break;
        }
        if (params.opId === "rfi.group_files") {
          const source = mockPath(values.source, "/Incoming");
          const destination = mockPath(values.destination, "/Grouped");
          const rows = [
            {
              _rowId: "file-group-0-0",
              enabled: true,
              rfiCode: "AZE-SUN-INT-QMS-RFI-CIV-1260",
              sourceFile: "AZE-SUN-INT-QMS-RFI-CIV-1260 SKETCH.pdf",
              method: "rfi.fileGroup.method.filename",
              confidence: 1,
            },
            {
              _rowId: "file-group-1-0",
              enabled: true,
              rfiCode: "AZE-SUN-INT-QMS-RFI-CIV-1261",
              sourceFile: "AZE-SUN-INT-QMS-RFI-CIV-1260_1261.eml",
              method: "rfi.fileGroup.method.mail",
              confidence: 1,
            },
          ];
          const plan = {
            planId: "demo-file-group-plan",
            opId: "rfi.group_files",
            warnings: [],
            meta: {
              sourceReadOnly: true,
              workerCount: 4,
              editor: {
                kind: "recordTable",
                layout: "compact",
                titleKey: "rfi.fileGroup.editor.title",
                rowCountKey: "rfi.fileGroup.editor.rowCount",
                groupField: "rfiCode",
                selectionGroupFields: ["rfiCode", "sourceFile"],
                selectionEnabled: true,
                bulkEnabled: true,
                requiredFields: ["enabled"],
                columns: [
                  { key: "enabled", labelKey: "rfi.fileGroup.column.enabled", kind: "bool", width: 64 },
                  { key: "rfiCode", labelKey: "rfi.fileGroup.column.rfiCode", kind: "readonly", width: 260, bulk: false },
                  { key: "sourceFile", labelKey: "rfi.fileGroup.column.sourceFile", kind: "readonly", width: 300, bulk: false },
                  { key: "method", labelKey: "rfi.fileGroup.column.method", kind: "readonly", display: "localizedKey", width: 120, bulk: false },
                  { key: "confidence", labelKey: "ocr.confidence", kind: "readonly", display: "percent", width: 112, bulk: false },
                ],
                rows,
              },
            },
            steps: rows.flatMap((row) => [
              {
                kind: "mkdir" as const,
                src: null,
                dst: `${destination}/${row.rfiCode}`,
                noteKey: "note.rfi.fileGroup.folderCreated",
                meta: { code: row.rfiCode },
              },
              {
                kind: "copy" as const,
                src: `${source}/${row.sourceFile}`,
                dst: `${destination}/${row.rfiCode}/${row.sourceFile}`,
                noteKey: "note.rfi.fileGroup.copy",
                meta: { rowId: row._rowId, code: row.rfiCode },
              },
            ]),
          } satisfies Plan;
          this.fileGroupPlan = plan;
          result = plan;
          break;
        }
        if (params.opId === "files.batch_print") {
          const files = Array.isArray(values.files)
            ? values.files.map((value) => mockPath(value, ""))
            : ["/Users/demo/Documents/first.pdf"];
          const rows = files.map((path, index) => ({
            _rowId: `file-${index + 1}`,
            enabled: true,
            status: "ready",
            file: path.split(/[\\/]/).pop() ?? path,
            path,
            format: path.split(".").pop()?.toUpperCase() ?? "FILE",
            pageRange: "all",
            printer: "Office Printer",
            paper: index ? "A3" : "A4",
            orientation: "auto",
            scaling: "fit",
            color: "printer",
            duplex: "printer",
            copies: 1,
          }));
          const plan = {
            planId: "demo-print-plan",
            opId: "files.batch_print",
            warnings: [],
            meta: {
              editor: {
                kind: "recordTable",
                titleKey: "print.editor.title",
                rowCountKey: "print.editor.rowCount",
                groupField: "format",
                selectionGroupFields: ["format"],
                selectionEnabled: true,
                bulkEnabled: true,
                columns: [
                  { key: "enabled", labelKey: "print.col.include", kind: "bool", width: 64 },
                  { key: "status", labelKey: "print.col.status", kind: "readonly", width: 90, bulk: false },
                  { key: "file", labelKey: "print.col.file", kind: "readonly", width: 260, bulk: false },
                  { key: "format", labelKey: "print.col.format", kind: "readonly", width: 76, bulk: false },
                  { key: "pageRange", labelKey: "print.col.pageRange", helpKey: "print.col.pageRange.help", kind: "text", width: 112, bulk: false },
                  { key: "printer", labelKey: "print.col.printer", kind: "choice", width: 180 },
                  { key: "paper", labelKey: "print.col.paper", kind: "choice", width: 88 },
                  { key: "orientation", labelKey: "print.col.orientation", kind: "choice", width: 110 },
                  { key: "scaling", labelKey: "print.col.scaling", kind: "choice", width: 110 },
                  { key: "color", labelKey: "print.col.color", kind: "choice", width: 100 },
                  { key: "duplex", labelKey: "print.col.duplex", kind: "choice", width: 116 },
                  { key: "copies", labelKey: "print.col.copies", kind: "int", width: 76 },
                ],
                choices: {
                  printer: ["Office Printer"],
                  paper: ["auto", "A4", "A3", "A5", "Letter", "Legal"],
                  orientation: ["auto", "portrait", "landscape"],
                  scaling: ["fit", "actual", "shrink"],
                  color: ["printer", "color", "grayscale"],
                  duplex: ["printer", "simplex", "long", "short"],
                },
                choiceLabels: {},
                rows,
              },
            },
            steps: rows.map((row) => ({
              kind: "print" as const,
              src: row.path,
              dst: row.path,
              noteKey: "note.print.queued",
              meta: {},
            })),
          } satisfies Plan;
          this.printPlan = plan;
          result = plan;
          break;
        }
        if (params.opId === "rfi.normalize_pdf_orientation") {
          result = {
            planId: "demo-orientation-plan",
            opId: "rfi.normalize_pdf_orientation",
            warnings: [],
            meta: {
              sourceReadOnly: true,
              workerCount: 4,
              verifiedPages: 24,
              rotatedPages: 3,
              actionableFiles: 2,
              manualReviewFiles: 0,
              scanIndex: {
                discovered: 8,
                reused: 6,
                analyzed: 2,
                failed: 0,
                priorInterruptedRuns: 0,
                forceRescan: Boolean(values.force_rescan),
              },
            },
            steps: [
              {
                kind: "pdf_page_rotate",
                src: `${mockPath(values.rfi_system_root, "/RFI System")}/MEC/RFI-0001/document.pdf`,
                dst: `${mockPath(values.review_root, "/Orientation Review")}/MEC/RFI-0001/document.pdf`,
                noteKey: "note.rfi.pdfOrientationNormalized",
                meta: {
                  rotations: [
                    { pageIndex: 1, fromRotation: 0, targetRotation: 90 },
                  ],
                },
              },
            ],
          } satisfies Plan;
          break;
        }
        if (params.opId === "rfi.log_add") {
          const rows = [
            {
              _rowId: "CIV-g1-r1",
              _sheet: "CIV",
              rfiNo: "AZE-SUN-INT-QMS-RFI-CIV-0008",
              itpNo: "AZE-SUN-INT-QMS-ITP-CIV-0005",
              itemNo: "CIV-5.2.1",
              description: "Civil inspection description",
              forms: "CON-CIV-001",
              tagNo: "N12",
              area: "North Site",
              rowNo: "",
              dateOfIssue: "15-Aug-26",
              inspectionDate: "16-Aug-26",
              startTime: "09:00",
              azfenType: "H",
              intecType: "W",
              location: "N3",
              subcontractor: "AZFEN",
            },
            {
              _rowId: "CIV-g2-r1",
              _sheet: "CIV",
              rfiNo: "AZE-SUN-INT-QMS-RFI-CIV-0009",
              itpNo: "AZE-SUN-INT-QMS-ITP-CIV-0005",
              itemNo: "CIV-5.2.1",
              description: "Civil inspection description",
              forms: "CON-CIV-001",
              tagNo: "N18",
              area: "North Site",
              rowNo: "",
              dateOfIssue: "15-Aug-26",
              inspectionDate: "16-Aug-26",
              startTime: "09:30",
              azfenType: "H",
              intecType: "W",
              location: "N3",
              subcontractor: "PDL",
            },
          ];
          result = {
            planId: "demo-rfi-plan",
            opId: "rfi.log_add",
            warnings: [],
            meta: {
              editor: {
                kind: "recordTable",
                layout: "compact",
                titleKey: "rfi.editor.title",
                groupField: "rfiNo",
                selectionGroupFields: ["_sheet", "rfiNo"],
                selectionEnabled: true,
                bulkEnabled: true,
                detailsEnabled: true,
                detailsDefaultVisible: false,
                duplicateEnabled: true,
                columns: [
                  { key: "_sheet", labelKey: "rfi.col.sheet", kind: "readonly", width: 42 },
                  { key: "rfiNo", labelKey: "rfi.col.rfiNo", kind: "readonly", width: 54, display: "suffix" },
                  { key: "itpNo", labelKey: "rfi.col.itpNo", kind: "choice", width: 50, display: "shortNumber" },
                  { key: "itemNo", labelKey: "rfi.col.itemNo", kind: "choice", width: 62, display: "itemSuffix" },
                  { key: "description", labelKey: "rfi.col.description", kind: "multiline", width: 360, detail: true },
                  { key: "forms", labelKey: "rfi.col.forms", kind: "multiline", width: 280, detail: true },
                  { key: "tagNo", labelKey: "rfi.col.tagNo", kind: "autocomplete", width: 92 },
                  { key: "area", labelKey: "rfi.col.area", kind: "autocomplete", width: 94 },
                  { key: "dateOfIssue", labelKey: "rfi.col.issueDate", kind: "compactDate", width: 116 },
                  { key: "inspectionDate", labelKey: "rfi.col.inspectionDate", kind: "compactDate", width: 116 },
                  { key: "startTime", labelKey: "rfi.col.startTime", kind: "text", width: 58 },
                  { key: "azfenType", labelKey: "rfi.col.azfenType", kind: "text", width: 130, detail: true },
                  { key: "intecType", labelKey: "rfi.col.intecType", kind: "text", width: 130, detail: true },
                  { key: "location", labelKey: "rfi.col.location", kind: "autocomplete", width: 104 },
                  { key: "subcontractor", labelKey: "rfi.col.subcontractor", kind: "autocomplete", width: 88 },
                ],
                choices: {
                  itpNo: ["AZE-SUN-INT-QMS-ITP-CIV-0005"],
                  itemNo: ["CIV-5.2.1"],
                },
                autocomplete: {
                  tagNo: ["N12", "N18"],
                  area: ["North Site", "South Site"],
                  location: ["N3"],
                  subcontractor: ["AZFEN", "PDL"],
                },
                rows,
              },
            },
            steps: [
              {
                kind: "workbook_update",
                src: mockPath(
                  values.workbook,
                  "/Users/demo/Documents/RFI Log.xlsx",
                ),
                dst: "/Users/demo/Documents/.docflow-backups/RFI Log.xlsx",
                noteKey: "note.rfi.workbookUpdate",
                meta: {},
              },
            ],
          } satisfies Plan;
          break;
        }
        result = {
          planId: "demo-plan",
          opId: String(params.opId),
          warnings: [],
          meta: {},
          steps: [
            {
              kind: "move",
              src: `${mockPath(values.source, "/Scans")}/20260718_163146.PDF`,
              dst: `${mockPath(values.dest, "/RFI")}/RFI-CIV-0064/RFI-CIV-0064-R0.pdf`,
              noteKey: "note.renamed",
              meta: { confidence: 1 },
            },
          ],
        } satisfies Plan;
        break;
      }
      case "engine/revise": {
        if (
          this.completeStatusPlan &&
          params.planId === this.completeStatusPlan.planId
        ) {
          const rows = Array.isArray(params.rows)
            ? params.rows.map((row) => ({ ...row }))
            : [];
          const editor = this.completeStatusPlan.meta.editor as Record<
            string,
            unknown
          >;
          const revised = {
            ...this.completeStatusPlan,
            planId: "demo-complete-status-plan-revised",
            meta: {
              ...this.completeStatusPlan.meta,
              editor: { ...editor, rows },
            },
            steps: rows.some((row) => row.enabled !== false)
              ? this.completeStatusPlan.steps
              : [],
          } satisfies Plan;
          this.completeStatusPlan = revised;
          result = revised;
          break;
        }
        if (this.fileGroupPlan && params.planId === this.fileGroupPlan.planId) {
          const rows = Array.isArray(params.rows)
            ? params.rows.map((row) => ({ ...row }))
            : [];
          const editor = this.fileGroupPlan.meta.editor as Record<string, unknown>;
          const enabledRows = rows.filter((row) => row.enabled !== false);
          const revised = {
            ...this.fileGroupPlan,
            planId: "demo-file-group-plan-revised",
            meta: {
              ...this.fileGroupPlan.meta,
              editor: { ...editor, rows },
            },
            steps: this.fileGroupPlan.steps.filter((step) => {
              const rowId = String(step.meta.rowId ?? "");
              if (rowId) return enabledRows.some((row) => row._rowId === rowId);
              const code = String(step.meta.code ?? "");
              return !code || enabledRows.some((row) => row.rfiCode === code);
            }),
          } satisfies Plan;
          this.fileGroupPlan = revised;
          result = revised;
          break;
        }
        if (!this.printPlan || params.planId !== this.printPlan.planId) {
          throw new Error("Unknown demo print plan");
        }
        const rows = Array.isArray(params.rows)
          ? params.rows.map((row) => ({ ...row }))
          : [];
        const editor = this.printPlan.meta.editor as Record<string, unknown>;
        const revised = {
          ...this.printPlan,
          planId: "demo-print-plan-revised",
          meta: {
            ...this.printPlan.meta,
            editor: { ...editor, rows },
          },
          steps: rows
            .filter((row) => row.enabled !== false)
            .map((row) => ({
              kind: "print" as const,
              src: String(row.path),
              dst: String(row.path),
              noteKey: "note.print.queued",
              meta: {},
            })),
        } satisfies Plan;
        this.printPlan = revised;
        result = revised;
        break;
      }
      case "engine/rfiReplyPreview":
        result = {
          planId: "demo-rfi-reply-plan",
          opId: "rfi.outlook_reply_draft",
          steps: [
            {
              kind: "outlook_reply_draft",
              src: "RFI number request",
              dst: null,
              noteKey: "step.rfi.outlookReplyDraft",
              meta: { displayOnly: true, sendAllowed: false },
            },
          ],
          warnings: [],
          meta: {
            replyPreview: {
              subject: "RFI number request",
              plainText:
                "RFI No.\tITP No.\r\nAZE-SUN-INT-QMS-RFI-CIV-0008\tCIV-0005",
              html: "<table><tr><td>AZE-SUN-INT-QMS-RFI-CIV-0008</td></tr></table><br><br>",
            },
          },
        } satisfies Plan;
        break;
      case "engine/apply":
        onProgress?.({ phase: "apply", done: 1, total: 1 });
        result = {
          applied: 1,
          failed: 0,
          journalId: "demo-journal",
          errors: [],
          cloudSynced: true,
        } satisfies OperationResult;
        break;
      case "engine/printQueueLatest":
        result = null;
        break;
      case "engine/printQueueDiscard":
        result = { status: "abandoned" };
        break;
      case "engine/cancel":
        result = { accepted: true };
        break;
      case "engine/reviewScan":
        result = [
          {
            selectionId: "demo-review-item-1",
            path: "/RFI/_review/scan-01.pdf",
            name: "scan-01.pdf",
            code: "RFI-CIV-0064",
            confidence: 0.9,
            targetExists: false,
          },
          {
            selectionId: "demo-review-item-2",
            path: "/RFI/_review/scan-02.pdf",
            name: "scan-02.pdf",
            code: "",
            confidence: 0,
            targetExists: false,
          },
        ] satisfies ReviewItem[];
        break;
      case "engine/reviewPreview":
        result = {
          planId: "demo-review-plan",
          opId: "rfi.review",
          warnings: [],
          meta: {},
          steps: [
            {
              kind: "move",
              src: "/RFI/_review/scan-01.pdf",
              dst: "/RFI/RFI-CIV-0064/RFI-CIV-0064-R0.pdf",
              noteKey: "note.renamed",
              meta: {},
            },
          ],
        } satisfies Plan;
        break;
      case "engine/undo":
        result = {
          applied: 1,
          failed: 0,
          journalId: "",
          errors: [],
        } satisfies OperationResult;
        break;
      case "history/list":
        result = {
          online: true,
          rows: [
            {
              when: "2026-08-24T00:00:00Z",
              opId: demoOperation.id,
              moves: 4,
              failed: 0,
              journalId: "local-1",
              steps: null,
              cloudId: "cloud-1",
              undone: false,
              undoAvailable: true,
              fromCloud: true,
              userEmail: demoAdmin.email,
            },
            {
              when: "2026-08-23T00:00:00Z",
              opId: demoOperation.id,
              moves: 1,
              failed: 0,
              journalId: "remote-only",
              steps: null,
              cloudId: "cloud-2",
              undone: false,
              undoAvailable: false,
              fromCloud: true,
              userEmail: "other@docflow.local",
            },
          ],
        } satisfies HistoryPayload;
        break;
      case "paths/select": {
        const multiple = params.paramName === "files";
        const paths = multiple
          ? [
              "/Users/demo/Documents/first.pdf",
              "/Users/demo/Documents/second.docx",
            ]
          : [
              params.paramName === "workbook"
                ? "/Users/demo/Documents/RFI Log.xlsx"
                : "/Users/demo/Documents/DocFlow",
            ];
        result = paths.map((displayPath, index) => ({
          selectionId: `demo-selection-${String(params.paramName)}-${index}`,
          displayPath,
          name: displayPath.split(/[\\/]/).pop() ?? displayPath,
        })) satisfies PathSelection[];
        break;
      }
      case "paths/reveal":
        result = { ok: true };
        break;
      case "recovery/list":
        result = {
          journals: [
            {
              journalId: "demo-recovery-journal",
              disposition: "outcome_unknown",
            },
          ],
          printQueue: {
            queueId: "demo-recovery-queue",
            status: "paused",
            createdAt: "2026-08-24T00:00:00Z",
            updatedAt: "2026-08-24T00:01:00Z",
            counts: { uncertain: 1 },
            resumable: 1,
            fileCount: 1,
          },
        } satisfies RecoveryPayload;
        break;
      default:
        throw new Error(`Mock method not implemented: ${String(method)}`);
    }
    return result as CommandResult<Method>;
  }
}

let sharedBridge: Bridge | null = null;

export function getBridge(): Bridge {
  if (!sharedBridge) {
    sharedBridge = window.__TAURI_INTERNALS__
      ? new TauriBridge()
      : new MockBridge();
  }
  return sharedBridge;
}

export function resetBridgeForTests(): void {
  if (!window.__TAURI_INTERNALS__) sharedBridge = null;
}
