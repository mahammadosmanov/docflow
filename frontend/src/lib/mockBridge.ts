import type { Bridge, ProgressHandler } from "./bridge";
import { demoFastCoverOperation } from "./demoOperations";
import {
  diagnosticsOperation,
  printFindOperation,
} from "./mockHandlers";
import {
  type MockPlanState,
  previewOperation,
} from "./mockPlanHandlers";
import { reviseOperation } from "./mockReviseHandler";
import {
  defaultPrefs,
  demoAdmin,
  demoOperationList,
  signedOut,
} from "./mockFixtures";
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
  OperationPreset,
  OperationResult,
  PathSelection,
  Plan,
  RecoveryPayload,
  ReplicaAutomationStatus,
  ReviewItem,
  WatchStatus,
} from "../types";

/** Browser-only bridge used by component tests and visual development. */
export class MockBridge implements Bridge {
  private auth: AuthState = signedOut;
  private prefs: AppPrefs = { ...defaultPrefs };
  private params: Record<string, unknown> = {};
  private presets: Record<string, OperationPreset[]> = {};
  private readonly plans: MockPlanState = {
    printPlan: null,
    fileGroupPlan: null,
    completeStatusPlan: null,
    fastScanPlan: null,
  };
  private watchEnabled: string[] = [];

  private watchStatus(): WatchStatus {
    return {
      operations: [
        "rfi.process_scans_fast_covers",
        "rfi.group_files",
        "rfi.stage_signed_first_pages",
        "rfi.complete_status_sync",
      ],
      enabled: this.watchEnabled,
      configured: [],
      pollSeconds: 300,
      paused: false,
      items: [],
    };
  }

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
          operations: demoOperationList,
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
      case "settings/presets":
        result = this.presets[String(params.opId)] ?? [];
        break;
      case "settings/savePreset": {
        const opId = String(params.opId);
        const name = String(params.name);
        const list = this.presets[opId] ?? [];
        const saved: OperationPreset = {
          id: `preset-${list.length + 1}-${name}`,
          name,
          values: (params.values as Record<string, unknown>) ?? {},
        };
        this.presets[opId] = params.replace
          ? [...list.filter((preset) => preset.name !== name), saved]
          : [...list, saved];
        result = saved;
        break;
      }
      case "settings/deletePreset":
        this.presets[String(params.opId)] = (
          this.presets[String(params.opId)] ?? []
        ).filter((preset) => preset.id !== params.presetId);
        result = { ok: true };
        break;
      case "support/diagnostics":
        result = diagnosticsOperation(params, onProgress);
        break;
      case "support/copyDiagnostics":
        result = { ok: true };
        break;
      case "support/activityLog":
        result = {
          fileName: "docflow-activity-log.jsonl",
          mediaType: "application/x-ndjson",
          content: "",
        };
        break;
      case "engine/preview":
        result = previewOperation(this.plans, params, onProgress);
        break;
      case "engine/revise":
        result = reviseOperation(this.plans, params, onProgress);
        break;
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
      case "rfi/printFind":
        result = printFindOperation(params, onProgress);
        break;
      case "history/list":
        result = {
          online: true,
          rows: [
            {
              when: "2026-08-24T00:00:00Z",
              opId: demoFastCoverOperation.id,
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
              opId: demoFastCoverOperation.id,
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
      case "watch/status":
        result = this.watchStatus();
        break;
      case "watch/update": {
        const opId = String(params.opId);
        if (typeof params.enabled === "boolean") {
          this.watchEnabled = params.enabled
            ? [...new Set([...this.watchEnabled, opId])]
            : this.watchEnabled.filter((id) => id !== opId);
        }
        result = this.watchStatus();
        break;
      }
      case "watch/open":
        throw new Error("error.watch.notReady");
      default:
        throw new Error(`Mock method not implemented: ${String(method)}`);
    }
    return result as CommandResult<Method>;
  }
}
