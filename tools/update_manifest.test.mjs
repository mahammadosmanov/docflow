import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { buildUpdateManifest } from "./update_manifest.mjs";

test("update manifest points Windows clients to the immutable signed release asset", () => {
  const manifest = buildUpdateManifest({
    version: "1.38.0",
    repository: "mahammadosmanov/docflow",
    tag: "v1.38.0",
    archiveName: "DocFlow_1.38.0_x64-setup.nsis.zip",
    signature: "minisign-signature",
    notes: "In-app update support.",
  });

  assert.equal(manifest.version, "1.38.0");
  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/mahammadosmanov/docflow/releases/download/v1.38.0/DocFlow_1.38.0_x64-setup.nsis.zip",
  );
  assert.equal(manifest.platforms["windows-x86_64"].signature, "minisign-signature");
});

test("manifest CLI writes once and refuses to replace an existing file", () => {
  const directory = mkdtempSync(join(tmpdir(), "docflow-update-manifest-"));
  try {
    const archive = "DocFlow_1.38.0_x64-setup.nsis.zip";
    const signaturePath = join(directory, `${archive}.sig`);
    const notesPath = join(directory, "notes.md");
    const outputPath = join(directory, "latest.json");
    writeFileSync(signaturePath, "signed payload\n");
    writeFileSync(notesPath, "Update notes\n");
    const script = resolve("tools/update_manifest.mjs");
    const args = [
      script,
      "--version", "1.38.0",
      "--repository", "mahammadosmanov/docflow",
      "--tag", "v1.38.0",
      "--archive", archive,
      "--signature", signaturePath,
      "--notes", notesPath,
      "--output", outputPath,
    ];

    execFileSync(process.execPath, args);
    assert.equal(JSON.parse(readFileSync(outputPath, "utf8")).version, "1.38.0");
    assert.throws(() => execFileSync(process.execPath, args, { stdio: "ignore" }));
    assert.equal(JSON.parse(readFileSync(outputPath, "utf8")).version, "1.38.0");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("update manifest rejects a tag that does not match its version", () => {
  assert.throws(
    () => buildUpdateManifest({
      version: "1.38.0",
      repository: "mahammadosmanov/docflow",
      tag: "v1.37.6",
      archiveName: "DocFlow_1.38.0_x64-setup.nsis.zip",
      signature: "signature",
      notes: "notes",
    }),
    /tag must match version/u,
  );
});
