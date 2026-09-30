import type {
  AppPrefs,
  AppUser,
  AuthState,
  Bootstrap,
  HistoryPayload,
  OperationResult,
  OperationPreset,
  PathSelection,
  Plan,
  PrintQueueSummary,
  ProgressEventHandler,
  RecoveryPayload,
  ReplicaAutomationStatus,
  ReviewDecision,
  ReviewItem,
  SupportBundlePreview,
} from "../types";

interface CommandSpec<Params, Result> {
  params: Params;
  result: Result;
}

type NoParams = undefined;
type Ok = { ok: true };

export interface FrontendCommandMap {
  "app/bootstrap": CommandSpec<NoParams, Bootstrap>;
  "auth/usernameAvailable": CommandSpec<{ username: string }, boolean>;
  "auth/signIn": CommandSpec<
    { identifier: string; password: string },
    AuthState
  >;
  "auth/signUp": CommandSpec<
    { email: string; password: string; username: string },
    Ok
  >;
  "auth/signOut": CommandSpec<NoParams, AuthState>;
  "auth/loadMe": CommandSpec<NoParams, AuthState>;
  "auth/setUsername": CommandSpec<{ username: string }, AuthState>;
  "auth/listUsers": CommandSpec<NoParams, AppUser[]>;
  "auth/setFlags": CommandSpec<
    { userId: string; allowed?: boolean; isAdmin?: boolean },
    Ok
  >;
  "settings/update": CommandSpec<Partial<AppPrefs>, AppPrefs>;
  "settings/reset": CommandSpec<NoParams, AppPrefs>;
  "settings/replicaStatus": CommandSpec<NoParams, ReplicaAutomationStatus>;
  "settings/params": CommandSpec<
    { opId: string },
    Record<string, unknown>
  >;
  "settings/saveParams": CommandSpec<
    { opId: string; values: Record<string, unknown> },
    Ok
  >;
  "settings/presets": CommandSpec<{ opId: string }, OperationPreset[]>;
  "settings/savePreset": CommandSpec<
    {
      opId: string;
      name: string;
      values: Record<string, unknown>;
      replace?: boolean;
    },
    OperationPreset
  >;
  "settings/deletePreset": CommandSpec<
    { opId: string; presetId: string },
    Ok
  >;
  "support/diagnostics": CommandSpec<NoParams, SupportBundlePreview>;
  "support/copyDiagnostics": CommandSpec<{ content: string }, Ok>;
  "engine/preview": CommandSpec<
    { opId: string; values: Record<string, unknown> },
    Plan
  >;
  "engine/apply": CommandSpec<
    { opId: string; planId: string },
    OperationResult
  >;
  "engine/cancel": CommandSpec<
    { planId?: string; opId?: string },
    { accepted: boolean }
  >;
  "engine/printQueueLatest": CommandSpec<NoParams, PrintQueueSummary | null>;
  "engine/printQueueDiscard": CommandSpec<
    { queueId: string },
    PrintQueueSummary
  >;
  "engine/printQueueResume": CommandSpec<{ queueId: string }, Plan>;
  "engine/revise": CommandSpec<
    { planId: string; rows: Record<string, unknown>[] },
    Plan
  >;
  "engine/copyTable": CommandSpec<
    { planId: string; sheet?: string },
    { count: number }
  >;
  "engine/rfiReplyPreview": CommandSpec<
    { planId: string; journalId: string },
    Plan
  >;
  "engine/reviewScan": CommandSpec<
    { destination: PathSelection },
    ReviewItem[]
  >;
  "engine/reviewPreview": CommandSpec<
    { destination: PathSelection; decisions: ReviewDecision[] },
    Plan
  >;
  "engine/undo": CommandSpec<
    {
      journalId: string;
      cloudId?: string | null;
      destination?: PathSelection;
    },
    OperationResult
  >;
  "history/list": CommandSpec<{ limit?: number }, HistoryPayload>;
  "paths/select": CommandSpec<
    { opId: string; paramName: string; droppedPaths?: string[] },
    PathSelection[]
  >;
  "paths/reveal": CommandSpec<{ selectionId: string }, Ok>;
  "recovery/list": CommandSpec<NoParams, RecoveryPayload>;
}

// BEGIN RUST-VERIFIED FRONTEND METHODS
export const FRONTEND_METHODS = [
  "app/bootstrap",
  "auth/usernameAvailable",
  "auth/signIn",
  "auth/signUp",
  "auth/signOut",
  "auth/loadMe",
  "auth/setUsername",
  "auth/listUsers",
  "auth/setFlags",
  "settings/update",
  "settings/reset",
  "settings/replicaStatus",
  "settings/params",
  "settings/saveParams",
  "settings/presets",
  "settings/savePreset",
  "settings/deletePreset",
  "support/diagnostics",
  "support/copyDiagnostics",
  "engine/preview",
  "engine/apply",
  "engine/cancel",
  "engine/printQueueLatest",
  "engine/printQueueDiscard",
  "engine/printQueueResume",
  "engine/revise",
  "engine/copyTable",
  "engine/rfiReplyPreview",
  "engine/reviewScan",
  "engine/reviewPreview",
  "engine/undo",
  "history/list",
  "paths/select",
  "paths/reveal",
  "recovery/list",
] as const satisfies readonly (keyof FrontendCommandMap)[];
// END RUST-VERIFIED FRONTEND METHODS

export type FrontendMethod = (typeof FRONTEND_METHODS)[number];
export type CommandParams<Method extends FrontendMethod> =
  FrontendCommandMap[Method]["params"];
export type CommandResult<Method extends FrontendMethod> =
  FrontendCommandMap[Method]["result"];
export type CommandArgs<Method extends FrontendMethod> =
  CommandParams<Method> extends undefined
    ? [params?: undefined, onProgress?: ProgressEventHandler]
    : [params: CommandParams<Method>, onProgress?: ProgressEventHandler];

type MissingMethods = Exclude<keyof FrontendCommandMap, FrontendMethod>;
const noMissingMethods: MissingMethods extends never ? true : never = true;
void noMissingMethods;
