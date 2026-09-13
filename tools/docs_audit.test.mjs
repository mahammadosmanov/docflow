import assert from "node:assert/strict";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { auditDocumentation } from "./docs_audit.mjs";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

test("repository documentation is complete and matches the operation roster", () => {
  assert.deepEqual(auditDocumentation(root), []);
});

test("documentation audit rejects broken maps and undocumented operations", () => {
  const fixture = mkdtempSync(join(tmpdir(), "docflow-docs-"));
  try {
    for (const path of ["AGENTS.md", "CLAUDE.md", "docs", "src-tauri/src/registry.rs"]) {
      cpSync(join(root, path), join(fixture, path), { recursive: true });
    }
    writeFileSync(
      join(fixture, "docs/README.md"),
      "# Documentation map\n\n[Missing](DOES_NOT_EXIST.md)\n",
    );
    writeFileSync(
      join(fixture, "docs/FEATURES.md"),
      "# Feature catalog\n\n| Operation | Outcome |\n|---|---|\n",
    );
    const failures = auditDocumentation(fixture).join("\n");
    assert.match(failures, /broken local link/);
    assert.match(failures, /registered operation is undocumented/);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});
