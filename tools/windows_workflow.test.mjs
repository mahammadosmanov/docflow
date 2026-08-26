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

test("Windows backend tests report every failing integration binary", () => {
  const source = readFileSync(resolve(root, "tools/build_windows.ps1"), "utf8");

  assert.match(
    source,
    /@\("test", "--manifest-path", "src-tauri\/Cargo\.toml", "--no-fail-fast"\)/u,
  );
});

test("Windows package smoke waits for GUI-subsystem processes", () => {
  for (const path of ["tools/build_windows.ps1", "tools/smoke_windows_installer.ps1"]) {
    const source = readFileSync(resolve(root, path), "utf8");
    const smokeBlock = source
      .split("$smoke = Start-Process", 2)[1]
      ?.split("if ($smoke.ExitCode", 1)[0];
    assert.ok(smokeBlock, path);
    assert.match(smokeBlock, /"--package-smoke"/u, path);
    assert.match(smokeBlock, /\)\s+-Wait\s+-PassThru\s*$/u, path);
  }

  const sourceBuild = readFileSync(resolve(root, "tools/build_windows.ps1"), "utf8");
  const installedSmoke = readFileSync(resolve(root, "tools/smoke_windows_installer.ps1"), "utf8");
  assert.doesNotMatch(sourceBuild, /Invoke-Checked \(Resolve-Path \$application\)\.Path/u);
  assert.doesNotMatch(installedSmoke, /& \$application --package-smoke/u);
});

test("manual Windows runs can deliver only to an exact unpublished draft", () => {
  const source = readFileSync(resolve(root, ".github/workflows/windows-release.yml"), "utf8");

  assert.match(source, /draft_delivery_tag:/u);
  assert.match(source, /if \(-not \$release\.draft\)/u);
  assert.match(source, /\$release\.target_commitish -ne \$env:GITHUB_SHA/u);
  assert.match(source, /gh release upload \$env:DRAFT_DELIVERY_TAG/u);
  assert.match(source, /releases\/\$\(\$release\.id\)\/assets/u);
});
