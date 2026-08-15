import { Child, Command } from "@tauri-apps/plugin-shell";

import type {
  AppPrefs,
  AppUser,
  AuthState,
  Bootstrap,
  HistoryPayload,
  OperationDef,
  OperationResult,
  Plan,
  ProgressEvent,
  ReviewItem,
} from "../types";

type ProgressHandler = (event: ProgressEvent) => void;

export interface Bridge {
  start(): Promise<void>;
  call<T>(
    method: string,
    params?: Record<string, unknown>,
    onProgress?: ProgressHandler,
  ): Promise<T>;
  stop(): Promise<void>;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
  onProgress?: ProgressHandler;
}

interface RpcMessage {
  type: "ready" | "response" | "progress";
  id?: string;
  requestId?: string;
  result?: unknown;
  event?: ProgressEvent;
  error?: {
    kind?: string;
    message?: string;
  };
}

export class SidecarBridge implements Bridge {
  private child: Child | null = null;
  private pending = new Map<string, PendingRequest>();
  private sequence = 0;
  private buffer = "";
  private ready: Promise<void> | null = null;
  private resolveReady: (() => void) | null = null;
  private rejectReady: ((reason: Error) => void) | null = null;

  async start(): Promise<void> {
    if (this.child && this.ready) {
      return this.ready;
    }
    this.ready = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    const command = Command.sidecar("binaries/docflow-sidecar");
    command.stdout.on("data", (chunk) => this.consume(chunk));
    command.stderr.on("data", (line) => {
      console.error(`[docflow-sidecar] ${line}`);
    });
    command.on("error", (message) => this.fail(new Error(message)));
    command.on("close", ({ code, signal }) => {
      this.fail(
        new Error(
          `DocFlow engine stopped unexpectedly (code ${code ?? "none"}, signal ${signal ?? "none"})`,
        ),
      );
    });
    try {
      this.child = await command.spawn();
    } catch (error) {
      this.fail(error instanceof Error ? error : new Error(String(error)));
    }
    const timeout = new Promise<never>((_resolve, reject) => {
      window.setTimeout(
        () => reject(new Error("DocFlow engine did not become ready")),
        120_000,
      );
    });
    return Promise.race([this.ready, timeout]);
  }

  async call<T>(
    method: string,
    params: Record<string, unknown> = {},
    onProgress?: ProgressHandler,
  ): Promise<T> {
    await this.start();
    if (!this.child) {
      throw new Error("DocFlow engine is not running");
    }
    const id = String(++this.sequence);
    const response = new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        onProgress,
      });
    });
    try {
      await this.child.write(`${JSON.stringify({ id, method, params })}\n`);
    } catch (error) {
      this.pending.delete(id);
      throw error;
    }
    return response;
  }

  async stop(): Promise<void> {
    const child = this.child;
    this.child = null;
    if (child) {
      await child.kill().catch(() => undefined);
    }
  }

  private consume(chunk: string): void {
    this.buffer += chunk;
    const lines = this.buffer.split(/\r?\n/);
    this.buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        this.route(JSON.parse(line) as RpcMessage);
      } catch {
        console.error(`[docflow-sidecar] invalid response: ${line}`);
      }
    }
  }

  private route(message: RpcMessage): void {
    if (message.type === "ready") {
      this.resolveReady?.();
      this.resolveReady = null;
      this.rejectReady = null;
      return;
    }
    const id =
      message.type === "progress" ? message.requestId : message.id;
    if (!id) return;
    const pending = this.pending.get(id);
    if (!pending) return;
    if (message.type === "progress") {
      if (message.event) pending.onProgress?.(message.event);
      return;
    }
    this.pending.delete(id);
    if (message.error) {
      pending.reject(
        new Error(message.error.message || message.error.kind || "Unknown error"),
      );
    } else {
      pending.resolve(message.result);
    }
  }

  private fail(error: Error): void {
    this.rejectReady?.(error);
    this.resolveReady = null;
    this.rejectReady = null;
    for (const request of this.pending.values()) request.reject(error);
    this.pending.clear();
    this.child = null;
  }
}

const demoOperation: OperationDef = {
  id: "rfi.process_scans",
  titleKey: "op.rfi.process_scans.title",
  descriptionKey: "op.rfi.process_scans.desc",
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

const demoRfiOperation: OperationDef = {
  id: "rfi.log_add",
  titleKey: "op.rfi.log_add.title",
  descriptionKey: "op.rfi.log_add.desc",
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

const demoPrintOperation: OperationDef = {
  id: "files.batch_print",
  titleKey: "op.files.batch_print.title",
  descriptionKey: "op.files.batch_print.desc",
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
      default: false,
      required: false,
      choices: [],
      choiceLabels: {},
      visibleWhen: {},
      helpKey: "param.rfi.scanSubfolders.help",
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
};

/** Browser-only bridge used by component tests and visual development. */
export class MockBridge implements Bridge {
  private auth: AuthState = signedOut;
  private prefs: AppPrefs = { ...defaultPrefs };
  private params: Record<string, unknown> = {};

  async start(): Promise<void> {}
  async stop(): Promise<void> {}

  async call<T>(
    method: string,
    params: Record<string, unknown> = {},
    onProgress?: ProgressHandler,
  ): Promise<T> {
    await new Promise((resolve) => window.setTimeout(resolve, 30));
    let result: unknown;
    switch (method) {
      case "app/bootstrap":
        result = {
          protocol: 1,
          version: "0.5.4-demo",
          auth: this.auth,
          prefs: this.prefs,
          operations: [
            demoOperation,
            demoRfiOperation,
            demoPrintOperation,
            demoSignedPagesOperation,
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
      case "auth/state":
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
      case "settings/get":
        result = this.prefs;
        break;
      case "settings/update":
        this.prefs = { ...this.prefs, ...params } as AppPrefs;
        result = this.prefs;
        break;
      case "settings/reset":
        this.prefs = { ...defaultPrefs };
        result = this.prefs;
        break;
      case "settings/params":
        result = this.params;
        break;
      case "settings/saveParams":
        this.params = (params.values as Record<string, unknown>) ?? {};
        result = { ok: true };
        break;
      case "engine/preview": {
        onProgress?.({ phase: "scan", done: 1, total: 1 });
        const values = params.values as Record<string, unknown>;
        if (params.opId === "files.batch_print") {
          const files = Array.isArray(values.files)
            ? values.files.map(String)
            : ["/Users/demo/Documents/first.pdf"];
          const rows = files.map((path, index) => ({
            _rowId: `file-${index + 1}`,
            enabled: true,
            status: "ready",
            file: path.split(/[\\/]/).pop() ?? path,
            path,
            format: path.split(".").pop()?.toUpperCase() ?? "FILE",
            printer: "Office Printer",
            paper: index ? "A3" : "A4",
            orientation: "auto",
            scaling: "fit",
            color: "printer",
            duplex: "printer",
            copies: 1,
          }));
          result = {
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
                titleKey: "rfi.editor.title",
                groupField: "rfiNo",
                selectionGroupFields: ["_sheet", "rfiNo"],
                selectionEnabled: true,
                bulkEnabled: true,
                detailsEnabled: true,
                detailsDefaultVisible: false,
                duplicateEnabled: true,
                columns: [
                  { key: "_sheet", labelKey: "rfi.col.sheet", kind: "readonly", width: 48 },
                  { key: "rfiNo", labelKey: "rfi.col.rfiNo", kind: "readonly", width: 72, display: "suffix" },
                  { key: "itpNo", labelKey: "rfi.col.itpNo", kind: "choice", width: 72, display: "suffix" },
                  { key: "itemNo", labelKey: "rfi.col.itemNo", kind: "choice", width: 76, display: "itemSuffix" },
                  { key: "description", labelKey: "rfi.col.description", kind: "multiline", width: 360, detail: true },
                  { key: "forms", labelKey: "rfi.col.forms", kind: "multiline", width: 280, detail: true },
                  { key: "tagNo", labelKey: "rfi.col.tagNo", kind: "autocomplete", width: 116 },
                  { key: "area", labelKey: "rfi.col.area", kind: "autocomplete", width: 116 },
                  { key: "dateOfIssue", labelKey: "rfi.col.issueDate", kind: "text", width: 90 },
                  { key: "inspectionDate", labelKey: "rfi.col.inspectionDate", kind: "text", width: 94 },
                  { key: "startTime", labelKey: "rfi.col.startTime", kind: "text", width: 68 },
                  { key: "azfenType", labelKey: "rfi.col.azfenType", kind: "text", width: 130, detail: true },
                  { key: "intecType", labelKey: "rfi.col.intecType", kind: "text", width: 130, detail: true },
                  { key: "location", labelKey: "rfi.col.location", kind: "autocomplete", width: 134 },
                  { key: "subcontractor", labelKey: "rfi.col.subcontractor", kind: "autocomplete", width: 110 },
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
                src: String(values.workbook || "/Users/demo/Documents/RFI Log.xlsx"),
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
              src: `${String(values.source || "/Scans")}/20260718_163146.PDF`,
              dst: `${String(values.dest || "/RFI")}/RFI-CIV-0064/RFI-CIV-0064-R0.pdf`,
              noteKey: "note.renamed",
              meta: { confidence: 1 },
            },
          ],
        } satisfies Plan;
        break;
      }
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
            path: "/RFI/_review/scan-01.pdf",
            name: "scan-01.pdf",
            code: "RFI-CIV-0064",
            targetExists: false,
          },
          {
            path: "/RFI/_review/scan-02.pdf",
            name: "scan-02.pdf",
            code: "",
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
              when: new Date().toISOString(),
              opId: demoOperation.id,
              moves: 4,
              failed: 0,
              journalId: "demo-journal",
              steps: null,
              cloudId: "demo-cloud",
              undone: false,
              fromCloud: true,
              userEmail: demoAdmin.email,
            },
          ],
        } satisfies HistoryPayload;
        break;
      default:
        throw new Error(`Mock method not implemented: ${method}`);
    }
    return result as T;
  }
}

let sharedBridge: Bridge | null = null;

export function getBridge(): Bridge {
  if (!sharedBridge) {
    sharedBridge = window.__TAURI_INTERNALS__
      ? new SidecarBridge()
      : new MockBridge();
  }
  return sharedBridge;
}

export function resetBridgeForTests(): void {
  if (!window.__TAURI_INTERNALS__) sharedBridge = null;
}
