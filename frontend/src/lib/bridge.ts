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
        const values = params.values as Record<string, string>;
        result = {
          planId: "demo-plan",
          opId: String(params.opId),
          warnings: [],
          meta: {},
          steps: [
            {
              kind: "move",
              src: `${values.source || "/Scans"}/20260718_163146.PDF`,
              dst: `${values.dest || "/RFI"}/RFI-CIV-0064/RFI-CIV-0064-R0.pdf`,
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
