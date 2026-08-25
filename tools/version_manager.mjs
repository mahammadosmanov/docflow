#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import {
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/u;
const RELEASE_STATUSES = new Set([
  "current",
  "supported",
  "superseded",
  "archived",
  "trial",
]);
const RELEASE_CHANNELS = new Set(["stable", "preview", "trial"]);

export class VersionError extends Error {}

export function parseVersion(value) {
  const normalized = String(value).trim();
  const match = normalized.match(SEMVER);
  if (!match) throw new VersionError(`invalid semantic version: ${JSON.stringify(value)}`);
  const prerelease = match[4] ? match[4].split(".") : [];
  if (prerelease.some((part) => /^0\d+$/u.test(part))) {
    throw new VersionError(
      `numeric prerelease identifiers cannot have leading zeroes: ${JSON.stringify(value)}`,
    );
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease,
  };
}

function readJson(path) {
  const value = JSON.parse(readFileSync(path, "utf8"));
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new VersionError(`expected a JSON object: ${path}`);
  }
  return value;
}

function sectionVersion(source, section, path) {
  const escaped = section.replaceAll(".", "\\.");
  const match = source.match(
    new RegExp(`^\\[${escaped}\\][\\s\\S]*?^version\\s*=\\s*"([^"]+)"`, "mu"),
  );
  if (!match) throw new VersionError(`missing [${section}].version in ${path}`);
  return match[1];
}

function cargoLockVersion(source) {
  const matches = [
    ...source.matchAll(
      /^\[\[package\]\]\s*\nname = "docflow"\s*\nversion = "([^"]+)"/gmu,
    ),
  ];
  if (matches.length !== 1) {
    throw new VersionError("expected one docflow package in src-tauri/Cargo.lock");
  }
  return matches[0][1];
}

export function canonicalVersion(root = REPOSITORY_ROOT) {
  const version = readFileSync(join(root, "VERSION"), "utf8").trim();
  parseVersion(version);
  return version;
}

export function manifestVersions(root = REPOSITORY_ROOT) {
  const packageJson = readJson(join(root, "package.json"));
  const packageLock = readJson(join(root, "package-lock.json"));
  const lockRoot = packageLock.packages?.[""];
  if (
    typeof packageLock.version !== "string" ||
    typeof lockRoot?.version !== "string" ||
    packageLock.version !== lockRoot.version
  ) {
    throw new VersionError("package-lock.json root versions disagree");
  }
  const cargoSource = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
  const cargoLockSource = readFileSync(join(root, "src-tauri/Cargo.lock"), "utf8");
  const tauri = readJson(join(root, "src-tauri/tauri.conf.json"));
  const versions = {
    "package.json": packageJson.version,
    "package-lock.json": packageLock.version,
    "src-tauri/Cargo.toml": sectionVersion(
      cargoSource,
      "package",
      "src-tauri/Cargo.toml",
    ),
    "src-tauri/Cargo.lock": cargoLockVersion(cargoLockSource),
    "src-tauri/tauri.conf.json": tauri.version,
  };
  for (const [path, version] of Object.entries(versions)) {
    if (typeof version !== "string") throw new VersionError(`missing version in ${path}`);
    parseVersion(version);
  }
  return versions;
}

export function releaseNotes(root, version) {
  parseVersion(version);
  const source = readFileSync(join(root, "CHANGELOG.md"), "utf8");
  const escaped = version.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const heading = new RegExp(`^##\\s+\\[?${escaped}\\]?(?:\\s+-.*)?\\s*$`, "mu");
  const match = heading.exec(source);
  if (!match) throw new VersionError(`CHANGELOG.md has no section for ${version}`);
  const start = match.index + match[0].length;
  const next = /^##\s+/mu.exec(source.slice(start));
  const end = next ? start + next.index : source.length;
  const notes = source.slice(start, end).trim();
  if (!notes) throw new VersionError(`CHANGELOG.md section for ${version} is empty`);
  return notes;
}

function validDate(value) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/u.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return !Number.isNaN(date.valueOf()) && date.toISOString().slice(0, 10) === value;
}

export function validateRepository(root = REPOSITORY_ROOT, { requireTag = false } = {}) {
  const errors = [];
  let current;
  try {
    current = canonicalVersion(root);
  } catch (error) {
    return [error.message];
  }
  try {
    for (const [path, version] of Object.entries(manifestVersions(root))) {
      if (version !== current) errors.push(`${path}: ${version} != VERSION ${current}`);
    }
  } catch (error) {
    errors.push(error.message);
  }
  try {
    const registry = readJson(join(root, "releases.json"));
    if (registry.schemaVersion !== 1) errors.push("releases.json: schemaVersion must be 1");
    if (registry.currentVersion !== current) {
      errors.push("releases.json: currentVersion does not match VERSION");
    }
    if (!Array.isArray(registry.releases) || registry.releases.length === 0) {
      errors.push("releases.json: releases must be a non-empty list");
    } else {
      const versions = new Set();
      const tags = new Set();
      let currentCount = 0;
      for (const [index, release] of registry.releases.entries()) {
        if (!release || typeof release !== "object" || Array.isArray(release)) {
          errors.push(`releases.json: release ${index} must be an object`);
          continue;
        }
        const version = release.version;
        if (typeof version !== "string") {
          errors.push(`releases.json: release ${index} has no version`);
          continue;
        }
        try {
          parseVersion(version);
        } catch (error) {
          errors.push(error.message);
        }
        if (versions.has(version)) errors.push(`releases.json: duplicate version ${version}`);
        versions.add(version);
        if (!RELEASE_STATUSES.has(release.status)) {
          errors.push(`releases.json: invalid status for ${version}`);
        }
        if (!RELEASE_CHANNELS.has(release.channel)) {
          errors.push(`releases.json: invalid channel for ${version}`);
        }
        if (!validDate(release.date)) errors.push(`releases.json: invalid date for ${version}`);
        if (typeof release.summary !== "string" || release.summary.trim() === "") {
          errors.push(`releases.json: missing summary for ${version}`);
        }
        const tag = release.tag;
        if (tag !== null && tag !== undefined) {
          if (typeof tag !== "string" || !tag.startsWith("v")) {
            errors.push(`releases.json: invalid tag for ${version}`);
          } else if (tag !== `v${version}`) {
            errors.push(`releases.json: tag ${tag} does not identify ${version}`);
          } else if (tags.has(tag)) {
            errors.push(`releases.json: duplicate tag ${tag}`);
          } else {
            tags.add(tag);
          }
        }
        if (release.status === "current") {
          currentCount += 1;
          if (version !== current) {
            errors.push(`releases.json: current release ${version} != VERSION ${current}`);
          }
          if (tag !== `v${version}`) {
            errors.push(`releases.json: current release ${version} requires tag v${version}`);
          }
        }
      }
      if (!versions.has(current)) errors.push(`releases.json: missing current version ${current}`);
      if (currentCount !== 1) errors.push("releases.json: exactly one release must be current");
    }
    releaseNotes(root, current);
  } catch (error) {
    errors.push(error.message);
  }
  if (requireTag) {
    try {
      const tags = execFileSync("git", ["tag", "--points-at", "HEAD"], {
        cwd: root,
        encoding: "utf8",
      })
        .trim()
        .split("\n")
        .filter(Boolean);
      if (!tags.includes(`v${current}`)) errors.push(`HEAD is not tagged v${current}`);
    } catch (error) {
      errors.push(`could not inspect Git tags: ${error.message}`);
    }
  }
  return errors;
}

function replaceSectionVersion(source, section, version, path) {
  const escaped = section.replaceAll(".", "\\.");
  const header = new RegExp(`^\\[${escaped}\\]\\s*$`, "mu").exec(source);
  if (!header) throw new VersionError(`could not find [${section}] in ${path}`);
  const start = header.index + header[0].length;
  const nextHeader = /^\[/mu.exec(source.slice(start));
  const end = nextHeader ? start + nextHeader.index : source.length;
  const block = source.slice(start, end);
  const updatedBlock = block.replace(/(^version\s*=\s*)"[^"]+"/mu, `$1"${version}"`);
  if (updatedBlock === block) {
    throw new VersionError(`could not update [${section}].version in ${path}`);
  }
  return source.slice(0, start) + updatedBlock + source.slice(end);
}

function replaceCargoLockVersion(source, version) {
  const pattern = /(^\[\[package\]\]\s*\nname = "docflow"\s*\nversion = )"[^"]+"/mu;
  if (!pattern.test(source)) {
    throw new VersionError("could not update docflow version in src-tauri/Cargo.lock");
  }
  return source.replace(pattern, `$1"${version}"`);
}

function jsonText(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function writeAtomic(path, content) {
  const temporary = join(dirname(path), `.${basename(path)}.${process.pid}.tmp`);
  writeFileSync(temporary, content, "utf8");
  renameSync(temporary, path);
}

export function setVersion(root, version) {
  parseVersion(version);
  const updates = new Map([[join(root, "VERSION"), `${version}\n`]]);
  const packagePath = join(root, "package.json");
  const packageJson = readJson(packagePath);
  packageJson.version = version;
  updates.set(packagePath, jsonText(packageJson));
  const lockPath = join(root, "package-lock.json");
  const packageLock = readJson(lockPath);
  if (!packageLock.packages?.[""]) {
    throw new VersionError("package-lock.json has no root package entry");
  }
  packageLock.version = version;
  packageLock.packages[""].version = version;
  updates.set(lockPath, jsonText(packageLock));
  const cargoPath = join(root, "src-tauri/Cargo.toml");
  updates.set(
    cargoPath,
    replaceSectionVersion(
      readFileSync(cargoPath, "utf8"),
      "package",
      version,
      "src-tauri/Cargo.toml",
    ),
  );
  const cargoLockPath = join(root, "src-tauri/Cargo.lock");
  updates.set(
    cargoLockPath,
    replaceCargoLockVersion(readFileSync(cargoLockPath, "utf8"), version),
  );
  const tauriPath = join(root, "src-tauri/tauri.conf.json");
  const tauri = readJson(tauriPath);
  tauri.version = version;
  updates.set(tauriPath, jsonText(tauri));
  for (const [path, content] of updates) writeAtomic(path, content);
}

export function listReleases(root = REPOSITORY_ROOT, { json = false } = {}) {
  const releases = readJson(join(root, "releases.json")).releases;
  if (json) return JSON.stringify(releases, null, 2);
  const lines = ["VERSION                 DATE         CHANNEL  STATUS       TAG"];
  for (const release of [...releases].reverse()) {
    lines.push(
      `${release.version.padEnd(23)} ${release.date.padEnd(12)} ${release.channel.padEnd(8)} ${release.status.padEnd(12)} ${release.tag ?? "-"}`,
      `  ${release.summary}`,
    );
  }
  return lines.join("\n");
}

function usage() {
  return "usage: node tools/version_manager.mjs check [--require-tag] | set <version> | list [--json] | notes <version>";
}

function main(argv) {
  const [command, ...rest] = argv;
  try {
    if (command === "check") {
      const errors = validateRepository(REPOSITORY_ROOT, {
        requireTag: rest.includes("--require-tag"),
      });
      if (errors.length > 0) {
        for (const error of errors) console.error(`ERROR: ${error}`);
        return 1;
      }
      console.log(`DocFlow ${canonicalVersion(REPOSITORY_ROOT)} version contract: OK`);
      return 0;
    }
    if (command === "set" && rest.length === 1) {
      setVersion(REPOSITORY_ROOT, rest[0]);
      console.log(
        `Updated runtime manifests to ${rest[0]}. Now update releases.json and CHANGELOG.md, then run check.`,
      );
      return 0;
    }
    if (command === "list") {
      console.log(listReleases(REPOSITORY_ROOT, { json: rest.includes("--json") }));
      return 0;
    }
    if (command === "notes" && rest.length === 1) {
      console.log(releaseNotes(REPOSITORY_ROOT, rest[0]));
      return 0;
    }
    console.error(usage());
    return 1;
  } catch (error) {
    console.error(`ERROR: ${error.message}`);
    return 1;
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  process.exitCode = main(process.argv.slice(2));
}
