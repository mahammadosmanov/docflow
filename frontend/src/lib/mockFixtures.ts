import type { AppPrefs, AppUser, AuthState, OperationDef } from "../types";
import {
  demoConcreteDocumentsOperation,
  demoFastCoverOperation,
  demoFileGroupOperation,
  demoPdfOrientationOperation,
  demoPrintOperation,
  demoRfiCompleteStatusOperation,
  demoRfiFollowUpOperation,
  demoRfiOperation,
  demoRfiSystemSyncOperation,
  demoSignedPagesOperation,
} from "./demoOperations";

export const signedOut: AuthState = {
  signedIn: false,
  allowed: false,
  isAdmin: false,
  session: null,
  user: null,
};

export const demoAdmin: AppUser = {
  id: "demo-user",
  email: "demo@docflow.local",
  username: "demo",
  display: "demo",
  allowed: true,
  isAdmin: true,
  createdAt: new Date().toISOString(),
};

export const defaultPrefs: AppPrefs = {
  locale: "tr",
  theme: "system",
  font: "system",
  fontScale: 1,
  compact: false,
  startWithWindows: false,
  replicaSyncEnabled: false,
};

export function mockPath(value: unknown, fallback: string): string {
  return value &&
    typeof value === "object" &&
    "displayPath" in value &&
    typeof value.displayPath === "string"
    ? value.displayPath
    : fallback;
}

export function normalizeMockRfiCode(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const captures =
    /^AZE-SUN-INT-QMS-RFI-(CIV|MEC|ELE|PRC)-([0-9]{1,4})(?![\s\S])/.exec(
      value,
    );
  return captures
    ? `AZE-SUN-INT-QMS-RFI-${captures[1]}-${captures[2].padStart(4, "0")}`
    : null;
}

export const demoOperationList: OperationDef[] = [
  demoFastCoverOperation,
  demoFileGroupOperation,
  demoRfiOperation,
  demoRfiFollowUpOperation,
  demoConcreteDocumentsOperation,
  demoPrintOperation,
  demoSignedPagesOperation,
  demoPdfOrientationOperation,
  demoRfiSystemSyncOperation,
  demoRfiCompleteStatusOperation,
];

export function isEmptyValue(value: unknown): boolean {
  return (
    value === undefined ||
    value === null ||
    value === "" ||
    (Array.isArray(value) && value.length === 0)
  );
}
