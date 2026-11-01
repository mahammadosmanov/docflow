import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("..", import.meta.url));
const script = join(root, "tools/updater_config.mjs");

test("updater config injects the trusted public key and static release endpoint", () => {
  const directory = mkdtempSync(join(tmpdir(), "docflow-updater-config-"));
  const output = join(directory, "tauri.updater.conf.json");
  const key = "untrusted comment: minisign public key: ABCD\nYWJjZA==";

  try {
    const result = spawnSync(process.execPath, [script, "--output", output], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, DOCFLOW_UPDATER_PUBLIC_KEY: `${key}\n` },
    });

    assert.equal(result.status, 0, result.stderr);
    const config = JSON.parse(readFileSync(output, "utf8"));
    assert.equal(config.bundle.createUpdaterArtifacts, true);
    assert.equal(config.plugins.updater.pubkey, key);
    assert.deepEqual(config.plugins.updater.endpoints, [
      "https://github.com/mahammadosmanov/docflow/releases/latest/download/latest.json",
    ]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("updater config fails closed without a public key or when output exists", () => {
  const directory = mkdtempSync(join(tmpdir(), "docflow-updater-config-"));
  const output = join(directory, "tauri.updater.conf.json");

  try {
    const missingKey = spawnSync(process.execPath, [script, "--output", output], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, DOCFLOW_UPDATER_PUBLIC_KEY: "" },
    });
    assert.notEqual(missingKey.status, 0);

    const generated = spawnSync(process.execPath, [script, "--output", output], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, DOCFLOW_UPDATER_PUBLIC_KEY: "test-key" },
    });
    assert.equal(generated.status, 0, generated.stderr);

    const overwrite = spawnSync(process.execPath, [script, "--output", output], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, DOCFLOW_UPDATER_PUBLIC_KEY: "replacement-key" },
    });
    assert.notEqual(overwrite.status, 0);
    assert.equal(JSON.parse(readFileSync(output, "utf8")).plugins.updater.pubkey, "test-key");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
