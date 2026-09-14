import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { FRONTEND_METHODS } from "./commands";

const repositoryRoot = process.cwd();
const read = (path: string) => readFileSync(resolve(repositoryRoot, path), "utf8");

describe("frontend authority boundary", () => {
  it("has no shell, process, filesystem, network, opener, or credential authority", () => {
    const packageJson = JSON.parse(read("package.json")) as {
      dependencies: Record<string, string>;
    };
    const capability = JSON.parse(
      read("src-tauri/capabilities/default.json"),
    ) as { permissions: string[] };
    const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json")) as {
      app: { security: { csp: string } };
    };
    const sources = [
      "frontend/src/App.tsx",
      "frontend/src/lib/bridge.ts",
      "frontend/src/lib/commands.ts",
      "frontend/src/lib/platform.ts",
      "frontend/src/main.tsx",
    ]
      .map(read)
      .join("\n");

    expect(repositoryRoot).toContain("docflow");
    expect(Object.keys(packageJson.dependencies)).not.toContain(
      "@tauri-apps/plugin-dialog",
    );
    expect(Object.keys(packageJson.dependencies)).not.toContain(
      "@tauri-apps/plugin-opener",
    );
    expect(capability.permissions).toEqual(["core:default"]);
    expect(tauriConfig.app.security.csp).toContain(
      "connect-src ipc: http://ipc.localhost",
    );
    expect(tauriConfig.app.security.csp).not.toMatch(
      /connect-src[^;]*https?:\/\/(?!ipc\.localhost)/,
    );
    expect(tauriConfig.app.security.csp).toContain(
      "img-src 'self' asset: http://asset.localhost data:",
    );
    expect(sources).not.toMatch(
      /@tauri-apps\/plugin-(?:dialog|fs|http|opener|shell)|child_process|localStorage|sessionStorage|\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource|accessToken|refreshToken|SUPABASE/i,
    );
    expect(
      [...sources.matchAll(/\binvoke[^\n(]*\(\s*["']([^"']+)["']/g)].map(
        (match) => match[1],
      ),
    ).toEqual(["frontend_call"]);
  });

  it("keeps TypeScript usage equal to the closed command roster", () => {
    const usage = ["frontend/src/App.tsx", "frontend/src/lib/platform.ts"]
      .map(read)
      .join("\n");
    const used = [...usage.matchAll(/\.call\(\s*["']([^"']+)["']/g)]
      .map((match) => match[1])
      .sort();
    const roster = [...FRONTEND_METHODS].sort();

    expect([...new Set(used)]).toEqual(roster);
    expect(usage).not.toMatch(/\.call\s*</);
    expect(read("frontend/src/lib/bridge.ts")).not.toContain("method: string");
  });

  it("keeps every user-facing locale key available in all four catalogs", () => {
    const locales = ["tr", "en", "ru", "az"].map((locale) =>
      Object.keys(
        JSON.parse(
          read(`frontend/src/locales/app_${locale}.arb`),
        ) as Record<string, unknown>,
      )
        .filter((key) => !key.startsWith("@"))
        .sort(),
    );

    for (const keys of locales.slice(1)) expect(keys).toEqual(locales[0]);
  });

  it("keeps the native Excel picker label in every locale catalog", () => {
    const label = "Excel (.xlsm, .xlsx)";
    expect(read("src-tauri/src/backend.rs")).toContain(`Some(("${label}"`);

    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      expect(catalog["dialog.filter.excelWorkbook"]).toBe(label);
    }
  });

  it("defines localized bootstrap failure title and retry text in every catalog", () => {
    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      expect(catalog["bootstrap.fatalTitle"]).toEqual(expect.any(String));
      expect(catalog["bootstrap.retry"]).toEqual(expect.any(String));
    }
  });

  it("defines the RFI Follow Up operation, required errors, and release canon", () => {
    const requiredKeys = [
      "op.rfi.follow_up_sync.title",
      "op.rfi.follow_up_sync.desc",
      "param.rfi.followUp.rfiLogWorkbook",
      "param.rfi.followUp.rfiLogWorkbook.help",
      "param.rfi.followUp.followUpWorkbook",
      "param.rfi.followUp.followUpWorkbook.help",
      "error.param.required.rfi_log_workbook",
      "error.param.required.follow_up_workbook",
      "error.rfi.followUp.workbookInvalid",
      "error.rfi.followUp.workbookLocked",
      "error.rfi.followUp.workbookChanged",
      "error.rfi.followUp.layoutAmbiguous",
      "error.rfi.followUp.layoutMissing",
      "error.rfi.followUp.patchVerification",
      "error.rfi.followUp.protected",
      "error.rfi.followUp.signedPackage",
      "error.rfi.followUp.limitExceeded",
      "error.rfi.followUp.noRowsSelected",
      "error.rfi.followUp.retainedStorageFull",
      "error.rfi.followUp.backupExists",
      "error.rfi.followUp.backupVerify",
      "error.rfi.followUp.recoveryRequired",
      "rfi.followUp.editor.title",
      "rfi.followUp.editor.rowCount",
      "rfi.followUp.column.enabled",
      "rfi.followUp.column.rfiNo",
      "rfi.followUp.column.sourceStatuses",
      "rfi.followUp.column.currentStatus",
      "rfi.followUp.column.proposedStatus",
      "rfi.followUp.column.disposition",
      "rfi.followUp.column.reason",
      "rfi.followUp.column.sheet",
      "rfi.followUp.column.row",
      "rfi.followUp.disposition.ready",
      "rfi.followUp.disposition.alreadySynced",
      "rfi.followUp.disposition.manualReview",
      "warn.rfi.followUp.manualRows",
      "warn.rfi.followUp.noRowsSelected",
      "warn.rfi.followUp.reason.targetRfiFormula",
      "warn.rfi.followUp.reason.targetRfiMerged",
      "warn.rfi.followUp.reason.blankRfi",
      "warn.rfi.followUp.reason.invalidRfi",
      "warn.rfi.followUp.reason.targetDuplicate",
      "warn.rfi.followUp.reason.sourceMissing",
      "warn.rfi.followUp.reason.sourceFormula",
      "warn.rfi.followUp.reason.sourceMerged",
      "warn.rfi.followUp.reason.sourceStatusBlank",
      "warn.rfi.followUp.reason.sourceStatusUnknown",
      "warn.rfi.followUp.reason.sourceConflict",
      "warn.rfi.followUp.reason.targetCellMissing",
      "warn.rfi.followUp.reason.targetFormula",
      "warn.rfi.followUp.reason.targetMerged",
      "warn.rfi.followUp.reason.targetStatusUnknown",
      "warn.rfi.followUp.reason.alreadySynced",
      "warn.rfi.followUp.reason.readySigned",
      "warn.rfi.followUp.reason.readyNotSigned",
    ];
    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      for (const key of requiredKeys) {
        expect(catalog[key], `${locale}:${key}`).toEqual(expect.any(String));
      }
    }
    const releaseSafety = read("docs/RELEASE_SAFETY.md");
    expect(releaseSafety).toContain("exact ten-operation registry");
    expect(releaseSafety).toContain("RFI Follow Up");
  });

  it("defines the RFI System Sync operation, required errors, and locale keys", () => {
    const requiredKeys = [
      "op.rfi.system_sync.title",
      "op.rfi.system_sync.desc",
      "param.rfi.systemSync.rfiSystemRoot",
      "param.rfi.systemSync.rfiSystemRoot.help",
      "param.rfi.systemSync.rfiLogWorkbook",
      "param.rfi.systemSync.rfiLogWorkbook.help",
      "param.rfi.systemSync.followUpWorkbook",
      "param.rfi.systemSync.followUpWorkbook.help",
      "error.param.required.rfi_system_root",
      "error.rfi.systemSync.workbookInvalid",
      "error.rfi.systemSync.workbookLocked",
      "warn.rfi.systemSync.noChangesNeeded",
      "note.rfi.systemSync.rfiLog",
      "note.rfi.systemSync.followUp",
      "note.rfi.systemSync.backup",
      "note.rfi.systemSync.restore",
      "rfi.systemSync.editor.title",
      "rfi.systemSync.editor.rowCount",
      "rfi.systemSync.column.enabled",
      "rfi.systemSync.column.discipline",
      "rfi.systemSync.column.rfiNo",
      "rfi.systemSync.column.worleyStatus",
      "rfi.systemSync.column.rfiLogAction",
      "rfi.systemSync.column.followUpAction",
      "rfi.systemSync.column.disposition",
      "rfi.systemSync.column.reason",
      "rfi.systemSync.disposition.ready",
      "rfi.systemSync.disposition.alreadySynced",
      "rfi.systemSync.disposition.unsigned",
    ];
    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      for (const key of requiredKeys) {
        expect(catalog[key], `${locale}:${key}`).toEqual(expect.any(String));
      }
    }
  });

  it("defines the separate Windows Outlook draft boundary in every locale", () => {
    const requiredKeys = [
      "op.rfi.outlook_reply_draft.title",
      "op.rfi.outlook_reply_draft.desc",
      "param.rfi.outlookReplyDraft.replyText",
      "param.rfi.outlookReplyDraft.replyText.help",
      "error.param.required.reply_text",
      "step.rfi.outlookReplyDraft",
      "outlookDraft.previewTitle",
      "outlookDraft.plainText",
      "outlookDraft.html",
      "warn.rfi.outlookDraft.unsent",
      "warn.rfi.outlookDraft.undoConditional",
      "warn.rfi.outlookDraft.undoUnavailable",
      "error.rfi.outlookDraft.unsupportedPlatform",
      "error.rfi.outlookDraft.selectionChanged",
      "error.rfi.outlookDraft.draftMissingOrChanged",
      "error.rfi.outlookDraft.undoUnavailable",
    ];
    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      for (const key of requiredKeys) {
        expect(catalog[key], `${locale}:${key}`).toEqual(expect.any(String));
      }
    }
  });
});
