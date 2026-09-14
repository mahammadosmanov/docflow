#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
} from "node:fs";
import { basename, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const IGNORED_SOURCE_DIRECTORIES = new Set([
  ".git",
  ".mypy_cache",
  ".pytest_cache",
  ".venv",
  "build",
  "dist",
  "frontend/dist",
  "legacy",
  "node_modules",
  "sidecar",
  "src-tauri/target",
]);
const FORBIDDEN_SOURCE_PATHS = [
  "docflow-sidecar.spec",
  "pyproject.toml",
  "requirements-dev.txt",
  "requirements.txt",
  "src/docflow",
  "tools/build_sidecar.py",
  "tools/ci_ocr_smoke.py",
  "tools/version_manager.py",
];
const ACTIVE_CONFIG_FILES = [
  "package.json",
  "package-lock.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "src-tauri/tauri.conf.json",
  "src-tauri/src/backend.rs",
  "src-tauri/src/lib.rs",
];
const FORBIDDEN_ACTIVE_CONFIG =
  /FallbackSupervisor|tauri[_-]plugin[_-]shell|externalBin|docflow-sidecar|PyInstaller|build_sidecar\.py|ci_ocr_smoke\.py|version_manager\.py/iu;
const FORBIDDEN_PACKAGE_NAME =
  /(^|[-_.])(docflow[-_.]?sidecar|python(?:\d+(?:\.\d+)*)?|libpython(?:\d+(?:\.\d+)*)?|pyinstaller|pyimod\d*|base_library)([-_.]|$)/iu;
const FORBIDDEN_PACKAGE_EXTENSION = new Set([
  ".py",
  ".pyc",
  ".pyd",
  ".pyo",
  ".pyz",
  ".whl",
]);

export function isForbiddenPackageName(name) {
  return FORBIDDEN_PACKAGE_NAME.test(name);
}

function normalizedRelative(root, path) {
  return relative(root, path).split(sep).join("/");
}

function isIgnoredSourcePath(relativePath) {
  return [...IGNORED_SOURCE_DIRECTORIES].some(
    (ignored) =>
      relativePath === ignored || relativePath.startsWith(`${ignored}/`),
  );
}

function walk(root, { source = false } = {}) {
  const files = [];
  const queue = [root];
  while (queue.length > 0) {
    const directory = queue.pop();
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      const relativePath = normalizedRelative(root, path);
      if (source && isIgnoredSourcePath(relativePath)) continue;
      if (entry.isSymbolicLink()) {
        files.push(path);
      } else if (entry.isDirectory()) {
        queue.push(path);
      } else if (entry.isFile()) {
        files.push(path);
      }
    }
  }
  return files.sort();
}

export function auditSource(root = REPOSITORY_ROOT) {
  const failures = [];
  for (const relativePath of FORBIDDEN_SOURCE_PATHS) {
    if (existsSync(join(root, relativePath))) {
      failures.push(`removed runtime path still exists: ${relativePath}`);
    }
  }
  for (const path of walk(root, { source: true })) {
    const relativePath = normalizedRelative(root, path);
    if (FORBIDDEN_PACKAGE_EXTENSION.has(extname(path).toLowerCase())) {
      failures.push(`Python file remains in active source: ${relativePath}`);
    }
  }
  for (const relativePath of ACTIVE_CONFIG_FILES) {
    const path = join(root, relativePath);
    if (!existsSync(path)) continue;
    const fullSource = readFileSync(path, "utf8");
    const source = relativePath === "src-tauri/src/backend.rs"
      ? fullSource.split("#[cfg(test)]", 1)[0]
      : fullSource;
    const match = source.match(FORBIDDEN_ACTIVE_CONFIG);
    if (match) {
      failures.push(
        `removed runtime marker ${JSON.stringify(match[0])} remains in ${relativePath}`,
      );
    }
  }
  return failures;
}

function inspectMachOLinks(path) {
  if (process.platform !== "darwin") return [];
  try {
    const output = execFileSync("otool", ["-L", path], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return /libpython|Python\.framework|docflow[-_.]?sidecar|PyInstaller/iu.test(
      output,
    )
      ? [`forbidden linked runtime in ${path}: ${output.trim()}`]
      : [];
  } catch {
    return [];
  }
}

export function auditPackage(root) {
  const resolvedRoot = resolve(root);
  if (!existsSync(resolvedRoot)) {
    return [`package path does not exist: ${resolvedRoot}`];
  }
  const failures = [];
  const paths = statSync(resolvedRoot).isDirectory()
    ? walk(resolvedRoot)
    : [resolvedRoot];
  for (const path of paths) {
    const name = basename(path);
    const extension = extname(name).toLowerCase();
    const relativePath = normalizedRelative(
      statSync(resolvedRoot).isDirectory() ? resolvedRoot : resolve(resolvedRoot, ".."),
      path,
    );
    if (
      FORBIDDEN_PACKAGE_EXTENSION.has(extension) ||
      isForbiddenPackageName(name)
    ) {
      failures.push(`forbidden packaged runtime path: ${relativePath}`);
    }
    if (!lstatSync(path).isSymbolicLink()) {
      failures.push(...inspectMachOLinks(path));
    }
  }
  if (statSync(resolvedRoot).isDirectory()) {
    const macExecutable = join(resolvedRoot, "Contents", "MacOS", "docflow");
    if (existsSync(macExecutable)) {
      for (const required of [
        join("Contents", "Resources", "resources", "tesseract", "tesseract"),
        join(
          "Contents",
          "Resources",
          "resources",
          "tesseract",
          "tessdata",
          "eng.traineddata",
        ),
        join(
          "Contents",
          "Resources",
          "resources",
          "tesseract",
          "tessdata",
          "osd.traineddata",
        ),
      ]) {
        if (!existsSync(join(resolvedRoot, required))) {
          failures.push(`required packaged OCR resource is missing: ${required}`);
        }
      }
    }
    const windowsExecutable = join(resolvedRoot, "docflow.exe");
    if (existsSync(windowsExecutable)) {
      for (const required of [
        join("resources", "tesseract", "tesseract.exe"),
        join("resources", "tesseract", "tessdata", "eng.traineddata"),
        join("resources", "tesseract", "tessdata", "osd.traineddata"),
      ]) {
        if (!existsSync(join(resolvedRoot, required))) {
          failures.push(`required packaged OCR resource is missing: ${required}`);
        }
      }
    }
  }
  return failures;
}

export function formatAudit(failures, label) {
  if (failures.length === 0) return `${label}: OK`;
  return `${label}: FAILED\n${failures.map((failure) => `- ${failure}`).join("\n")}`;
}

function main(argv) {
  const [mode, requestedPath] = argv;
  const failures =
    mode === "source"
      ? auditSource(requestedPath ? resolve(requestedPath) : REPOSITORY_ROOT)
      : mode === "package" && requestedPath
        ? auditPackage(requestedPath)
        : ["usage: node tools/package_audit.mjs source [root] | package <path>"];
  const label = mode === "package" ? "DocFlow package audit" : "DocFlow source audit";
  const output = formatAudit(failures, label);
  (failures.length === 0 ? console.log : console.error)(output);
  return failures.length === 0 ? 0 : 1;
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  process.exitCode = main(process.argv.slice(2));
}
