import assert from "node:assert/strict";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { auditDocumentation } from "./docs_audit.mjs";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

test("repository documentation is complete", () => {
  assert.deepEqual(auditDocumentation(root), []);
});

test("documentation audit rejects broken maps", () => {
  const fixture = mkdtempSync(join(tmpdir(), "docflow-docs-"));
  try {
    for (const path of ["AGENTS.md", "CLAUDE.md", "docs"]) {
      cpSync(join(root, path), join(fixture, path), { recursive: true });
    }
    writeFileSync(
      join(fixture, "docs/README.md"),
      "# Documentation map\n\n[Missing](DOES_NOT_EXIST.md)\n",
    );
    const failures = auditDocumentation(fixture).join("\n");
    assert.match(failures, /broken local link/);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});
