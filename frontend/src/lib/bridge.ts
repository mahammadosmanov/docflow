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
  OperationResult,
  PathSelection,
  Plan,
  ProgressEvent,
  RecoveryPayload,
  ReplicaAutomationStatus,
  ReviewItem,
  SupportBundlePreview,
  RfiPrintFindResult,
  WatchStatus,
} from "../types";


export type ProgressHandler = (event: ProgressEvent) => void;

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


export { MockBridge } from "./mockBridge";
import { MockBridge } from "./mockBridge";

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
