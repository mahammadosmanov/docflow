import assert from "node:assert/strict";
import {
  cpSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  canonicalVersion,
  manifestVersions,
  nextVersion,
  parseVersion,
  releaseNotes,
  setVersion,
  validateRepository,
} from "./version_manager.mjs";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

function repositoryFixture() {
  const temporary = mkdtempSync(join(tmpdir(), "docflow-version-"));
  for (const path of [
    "VERSION",
    "flutter/app/lib/engine/backend.dart",
    "releases.json",
    "CHANGELOG.md",
    "README.md",
    "docs/PRODUCT.md",
    "docs/ROADMAP.md",
  ]) {
    cpSync(join(root, path), join(temporary, path), { recursive: true });
  }
  return temporary;
}

test("repository version contract is consistent for the Flutter engine", () => {
  assert.deepEqual(validateRepository(root), []);
  const version = canonicalVersion(root);
  for (const manifestVersion of Object.values(manifestVersions(root))) {
    assert.equal(manifestVersion, version);
  }
});

test("set updates every remaining runtime manifest", () => {
  const fixture = repositoryFixture();
  try {
    setVersion(fixture, "1.2.3-rc.1");
    assert.equal(canonicalVersion(fixture), "1.2.3-rc.1");
    assert.deepEqual(new Set(Object.values(manifestVersions(fixture))), new Set(["1.2.3-rc.1"]));
    assert.match(readFileSync(join(fixture, "README.md"), "utf8"), /^DocFlow 1\.2\.3-rc\.1\b/mu);
    assert.match(
      readFileSync(join(fixture, "docs/PRODUCT.md"), "utf8"),
      /^Current product version: \*\*1\.2\.3-rc\.1\*\*\./mu,
    );
    assert.match(
      readFileSync(join(fixture, "docs/ROADMAP.md"), "utf8"),
      /^## Now — 1\.2\.3-rc\.1 reliability and Windows certification$/mu,
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("validation rejects stale human-facing current-version markers", () => {
  const fixture = repositoryFixture();
  try {
    const roadmapPath = join(fixture, "docs/ROADMAP.md");
    writeFileSync(
      roadmapPath,
      readFileSync(roadmapPath, "utf8").replace(/^## Now — .*$/mu, "## Now — 0.1 stale"),
    );
    assert.match(validateRepository(fixture).join("\n"), /docs\/ROADMAP\.md: current-version marker/);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("SemVer rejects numeric prerelease identifiers with leading zeroes", () => {
  assert.throws(() => parseVersion("1.2.3-rc.01"), /leading zeroes/);
  assert.deepEqual(parseVersion("1.2.3-rc.1").prerelease, ["rc", "1"]);
});

test("next version follows the project change classification", () => {
  assert.equal(nextVersion("0.7.0-rc.11", "fix"), "0.7.1");
  assert.equal(nextVersion("0.7.0-rc.11", "feature"), "0.8.0");
  assert.equal(nextVersion("0.7.0-rc.11", "breaking"), "1.0.0");
  assert.equal(nextVersion("0.8.0", "fix"), "0.8.1");
  assert.equal(nextVersion("0.8.0", "feature"), "0.9.0");
  assert.throws(() => nextVersion("0.8.0", "cosmetic"), /change type/);
});

test("validation rejects mismatched catalog tags and impossible dates", () => {
  const fixture = repositoryFixture();
  try {
    const registryPath = join(fixture, "releases.json");
    const registry = JSON.parse(readFileSync(registryPath, "utf8"));
    const current = registry.releases.find((release) => release.status === "current");
    current.tag = "v9.9.9";
    current.date = "2026-02-31";
    writeFileSync(registryPath, `${JSON.stringify(registry, null, 2)}\n`);
    const errors = validateRepository(fixture).join("\n");
    assert.match(errors, /does not identify/);
    assert.match(errors, /invalid date/);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("release notes never include neighboring sections", () => {
  const notes = releaseNotes(root, canonicalVersion(root));
  assert.ok(notes.length > 0);
  assert.doesNotMatch(notes, /^##\s+/mu);
});
