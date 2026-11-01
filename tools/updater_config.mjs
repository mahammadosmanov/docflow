#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const endpoint =
  "https://github.com/mahammadosmanov/docflow/releases/latest/download/latest.json";

function parseOutputPath(args) {
  const index = args.indexOf("--output");
  if (index < 0 || !args[index + 1] || args[index + 2]) {
    throw new Error("usage: node tools/updater_config.mjs --output <new-path>");
  }
  return resolve(args[index + 1]);
}

export function writeUpdaterConfig(outputPath, publicKey) {
  if (typeof publicKey !== "string" || !publicKey.trim()) {
    throw new Error("DOCFLOW_UPDATER_PUBLIC_KEY is required");
  }

  const configPath = resolve(root, "src-tauri/tauri.updater.conf.json");
  const config = JSON.parse(readFileSync(configPath, "utf8"));
  config.plugins = {
    ...config.plugins,
    updater: {
      pubkey: publicKey.trim(),
      endpoints: [endpoint],
    },
  };

  writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
    mode: 0o600,
  });
}

try {
  writeUpdaterConfig(parseOutputPath(process.argv.slice(2)), process.env.DOCFLOW_UPDATER_PUBLIC_KEY);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
