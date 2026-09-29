#!/usr/bin/env node

import { existsSync, readFileSync, realpathSync } from "node:fs";
import { dirname, join, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const REQUIRED_DOCUMENTS = [
  "AGENTS.md",
  "CLAUDE.md",
  "docs/README.md",
  "docs/PRODUCT.md",
  "docs/PRD.md",
  "docs/FEATURES.md",
  "docs/RFI_DOMAIN.md",
  "docs/SAFETY_AND_STATE.md",
  "docs/EDITOR_CONTRACT.md",
  "docs/ARCHITECTURE.md",
  "docs/IPC_PROTOCOL.md",
  "docs/TRACEABILITY.md",
  "docs/DECISIONS.md",
  "docs/KNOWN_ISSUES.md",
  "docs/RELEASE_SAFETY.md",
  "docs/WINDOWS_RUNBOOK.md",
];

function normalized(path) {
  return normalize(path).replaceAll("\\", "/");
}

export function auditDocumentation(root = REPOSITORY_ROOT) {
  const failures = [];
  for (const path of REQUIRED_DOCUMENTS) {
    if (!existsSync(join(root, path))) failures.push(`required document is missing: ${path}`);
  }
  if (failures.length > 0) return failures;

  const mapPath = join(root, "docs/README.md");
  const map = readFileSync(mapPath, "utf8");
  for (const match of map.matchAll(/\[[^\]]+\]\(([^)#]+)(?:#[^)]+)?\)/gu)) {
    const target = resolve(dirname(mapPath), match[1]);
    const relativeTarget = normalized(relative(root, target));
    if (relativeTarget.startsWith("../") || !existsSync(target)) {
      failures.push(`docs/README.md has a broken local link: ${match[1]}`);
    }
  }

  const registry = readFileSync(join(root, "src-tauri/src/registry.rs"), "utf8");
  const registered = new Set(
    [...registry.matchAll(/^\s*"((?:rfi|files)\.[a-z0-9_]+)",$/gmu)].map(
      (match) => match[1],
    ),
  );
  const features = readFileSync(join(root, "docs/FEATURES.md"), "utf8");
  const documented = new Set(
    [...features.matchAll(/^\| `((?:rfi|files)\.[a-z0-9_]+)` \|/gmu)].map(
      (match) => match[1],
    ),
  );
  for (const id of registered) {
    if (!documented.has(id)) failures.push(`registered operation is undocumented: ${id}`);
  }
  for (const id of documented) {
    if (!registered.has(id)) failures.push(`documented operation is not registered: ${id}`);
  }
  if (registered.size !== 13) {
    failures.push(`expected exactly 13 registered operations, found ${registered.size}`);
  }

  const prd = readFileSync(join(root, "docs/PRD.md"), "utf8");
  const traceability = readFileSync(join(root, "docs/TRACEABILITY.md"), "utf8");
  for (let index = 1; index <= 25; index += 1) {
    const id = `DF-${String(index).padStart(3, "0")}`;
    if (!prd.includes(`**${id} `)) failures.push(`PRD requirement is missing: ${id}`);
    if (!traceability.includes(id)) failures.push(`requirement has no traceability owner: ${id}`);
  }

  return failures;
}

function main() {
  const failures = auditDocumentation();
  if (failures.length > 0) {
    for (const failure of failures) console.error(`ERROR: ${failure}`);
    return 1;
  }
  console.log("DocFlow documentation contract: OK");
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  process.exitCode = main();
}
