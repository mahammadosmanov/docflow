#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function checksum(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function main(argv) {
  if (argv.length === 0) {
    console.error("usage: node tools/checksum.mjs <artifact> [...]");
    return 1;
  }
  for (const requested of argv) {
    const path = resolve(requested);
    const line = `${checksum(path)}  ${basename(path)}\n`;
    const output = `${path}.sha256`;
    writeFileSync(output, line, "ascii");
    console.log(output);
  }
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  process.exitCode = main(process.argv.slice(2));
}
