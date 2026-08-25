import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { checksum } from "./checksum.mjs";

test("checksum uses canonical lowercase SHA-256", () => {
  const root = mkdtempSync(join(tmpdir(), "docflow-checksum-"));
  try {
    const path = join(root, "artifact.bin");
    writeFileSync(path, "DocFlow\n");
    assert.equal(
      checksum(path),
      "42c51d62cb1006bde6fca62396b7853b55f1c9610876d39bbe094d251d33d208",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
