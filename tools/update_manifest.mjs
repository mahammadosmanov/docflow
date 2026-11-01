#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/u;

export function buildUpdateManifest({ version, repository, tag, archiveName, signature, notes }) {
  if (!VERSION_PATTERN.test(version ?? "")) throw new Error("version must be semantic version");
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository ?? "")) {
    throw new Error("repository must be owner/name");
  }
  if (tag !== `v${version}`) throw new Error("tag must match version");
  if (!/^DocFlow_[A-Za-z0-9.+-]+_x64-setup\.nsis\.zip$/u.test(archiveName ?? "")) {
    throw new Error("archive must be the x64 NSIS updater bundle");
  }
  if (typeof signature !== "string" || !signature.trim()) {
    throw new Error("signature is required");
  }
  if (typeof notes !== "string" || !notes.trim()) throw new Error("release notes are required");

  return {
    version,
    notes: notes.trim(),
    platforms: {
      "windows-x86_64": {
        url: `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(archiveName)}`,
        signature: signature.trim(),
      },
    },
  };
}

function args(values) {
  const result = {};
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index];
    if (!key?.startsWith("--") || !values[index + 1] || result[key.slice(2)]) {
      throw new Error("expected unique --name value arguments");
    }
    result[key.slice(2)] = values[index + 1];
  }
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const input = args(process.argv.slice(2));
    const manifest = buildUpdateManifest({
      version: input.version,
      repository: input.repository,
      tag: input.tag,
      archiveName: input.archive,
      signature: readFileSync(input.signature, "utf8"),
      notes: readFileSync(input.notes, "utf8"),
    });
    writeFileSync(input.output, `${JSON.stringify(manifest, null, 2)}\n`, { flag: "wx" });
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
