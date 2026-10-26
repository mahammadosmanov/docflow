import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const TYPES = "frontend/src/types.ts";
const RUST_ROOT = "src-tauri/src";

// Rust DTOs sent to the webview are serde camelCase. Names here map a Rust
// struct to the TS interface it should mirror; both sides are compared
// field-for-field, so a renamed or missing field fails the gate.
const PAIRS = {
  ParamDef: "ParamDef",
  OperationDef: "OperationDef",
  OperationPreset: "OperationPreset",
  Step: "Step",
  Warning: "Warning",
  Plan: "Plan",
  OperationResult: "OperationResult",
  PrintQueueSummary: "PrintQueueSummary",
  AppUser: "AppUser",
  AuthState: "AuthState",
  AppPrefs: "AppPrefs",
  Bootstrap: "Bootstrap",
  SupportBundlePreview: "SupportBundlePreview",
  ProgressEvent: "ProgressEvent",
  PathSelection: "PathSelection",
  RfiPrintCandidate: "RfiPrintCandidate",
  RfiPrintResolution: "RfiPrintResolution",
  RfiPrintFindResult: "RfiPrintFindResult",
  HistoryRow: "HistoryRow",
  HistoryPayload: "HistoryPayload",
  RecoveryEntry: "RecoveryEntry",
  RecoveryPayload: "RecoveryPayload",
  FrontendReviewItem: "ReviewItem",
  FrontendReviewDecision: "ReviewDecision",
};

function rustFiles(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return rustFiles(path);
    return path.endsWith(".rs") ? [path] : [];
  });
}

// Returns the body text between the first `{` after `header` and its matching `}`.
function braceBody(source, header) {
  const start = source.indexOf(header);
  if (start < 0) return null;
  const open = source.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  return null;
}

// Top-level field names only: nested `{}` bodies are skipped. Rust separates
// fields with commas and TS interfaces with semicolons.
function topLevelFields(body, { fieldPattern, separator }) {
  let depth = 0;
  let line = "";
  const entries = [];
  for (const ch of body) {
    if (ch === "{" || ch === "(" || ch === "<") depth += 1;
    if (ch === "}" || ch === ")" || ch === ">") depth -= 1;
    if (separator.test(ch) && depth === 0) {
      entries.push(line);
      line = "";
    } else {
      line += ch;
    }
  }
  entries.push(line);
  return entries
    .map((entry) => entry.replace(/#\[[^\]]*\]/g, "").trim())
    .map((entry) => entry.match(fieldPattern))
    .filter(Boolean)
    .map((match) => match[1]);
}

const camel = (name) => name.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

test("TS wire types mirror the serde camelCase DTO fields", () => {
  const rustSources = rustFiles(RUST_ROOT).map((path) => readFileSync(path, "utf8"));
  const tsSource = readFileSync(TYPES, "utf8");
  for (const [rustName, tsName] of Object.entries(PAIRS)) {
    const rustBody = rustSources
      .map((source) => braceBody(source, `struct ${rustName} `))
      .find(Boolean);
    assert.ok(rustBody, `Rust struct ${rustName} not found`);
    const tsBody = braceBody(tsSource, `export interface ${tsName} `);
    assert.ok(tsBody, `TS interface ${tsName} not found`);
    const rustFields = topLevelFields(rustBody, {
      fieldPattern: /^pub(?:\([^)]*\))?\s+(\w+)\s*:/,
      separator: /,/,
    })
      .map(camel)
      .sort();
    const tsFields = topLevelFields(tsBody, {
      fieldPattern: /^(\w+)\??\s*:/,
      separator: /;/,
    }).sort();
    assert.deepEqual(tsFields, rustFields, `${tsName} drifted from ${rustName}`);
  }
});
