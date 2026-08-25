import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { auditPackage } from "./package_audit.mjs";

test("package audit rejects Python, PyInstaller, and sidecar payloads", () => {
  const root = mkdtempSync(join(tmpdir(), "docflow-package-audit-"));
  try {
    mkdirSync(join(root, "Contents", "Resources", "runtime"), {
      recursive: true,
    });
    for (const name of [
      "module.py",
      "module.pyc",
      "libpython3.12.dylib",
      "base_library.zip",
      "docflow-sidecar",
    ]) {
      writeFileSync(join(root, "Contents", "Resources", "runtime", name), "x");
    }

    const failures = auditPackage(root).join("\n");
    assert.match(failures, /module\.py/);
    assert.match(failures, /module\.pyc/);
    assert.match(failures, /libpython3\.12\.dylib/);
    assert.match(failures, /base_library\.zip/);
    assert.match(failures, /docflow-sidecar/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("package audit accepts a Rust app and native OCR resources", () => {
  const root = mkdtempSync(join(tmpdir(), "docflow-package-audit-"));
  try {
    mkdirSync(join(root, "Contents", "MacOS"), { recursive: true });
    mkdirSync(join(root, "Contents", "Resources", "tesseract"), {
      recursive: true,
    });
    mkdirSync(join(root, "Contents", "Resources", "resources", "tesseract", "tessdata"), {
      recursive: true,
    });
    writeFileSync(join(root, "Contents", "MacOS", "docflow"), "rust executable");
    writeFileSync(
      join(root, "Contents", "Resources", "resources", "tesseract", "tesseract"),
      "native OCR executable",
    );
    writeFileSync(
      join(
        root,
        "Contents",
        "Resources",
        "resources",
        "tesseract",
        "tessdata",
        "eng.traineddata",
      ),
      "fixture",
    );

    assert.deepEqual(auditPackage(root), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("package audit rejects a DocFlow bundle without native OCR", () => {
  const root = mkdtempSync(join(tmpdir(), "docflow-package-audit-"));
  try {
    mkdirSync(join(root, "Contents", "MacOS"), { recursive: true });
    writeFileSync(join(root, "Contents", "MacOS", "docflow"), "rust executable");

    assert.match(auditPackage(root).join("\n"), /required packaged OCR resource/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
