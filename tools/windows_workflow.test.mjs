import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

test("Windows installer smoke launches the installed GUI before package verification", () => {
  const source = readFileSync(resolve(root, "tools/smoke_windows_installer.ps1"), "utf8");

  assert.match(source, /Start-Process\s+-FilePath\s+\$application\s+-PassThru/u);
  assert.match(source, /if\s*\(\$gui\.HasExited\)/u);
  assert.match(source, /Stop-Process\s+-Id\s+\$gui\.Id\s+-Force/u);
});

test("release tools use fileURLToPath instead of URL pathname on Windows", () => {
  for (const path of [
    "tools/checksum.mjs",
    "tools/license_audit.mjs",
    "tools/package_audit.mjs",
    "tools/version_manager.mjs",
    "tools/version_manager.test.mjs",
  ]) {
    const source = readFileSync(resolve(root, path), "utf8");
    assert.doesNotMatch(source, /new URL\([^\n]*import\.meta\.url\)\.pathname/u, path);
  }
});

test("CI workflows pin the verified Rust toolchain", () => {
  for (const path of [
    ".github/workflows/quality.yml",
    ".github/workflows/windows-release.yml",
  ]) {
    const source = readFileSync(resolve(root, path), "utf8");
    assert.match(
      source,
      /uses:\s+dtolnay\/rust-toolchain@master\s+with:\s+toolchain:\s+"1\.97\.1"\s+components:\s+rustfmt, clippy/u,
      path,
    );
    assert.doesNotMatch(source, /dtolnay\/rust-toolchain@stable/u, path);
  }
});
