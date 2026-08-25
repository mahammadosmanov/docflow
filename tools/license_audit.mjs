#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const REJECTED_LICENSE = /AGPL|Affero|LicenseRef-commercial|commercial license/iu;

function hostTarget() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = output.split("\n").find((candidate) => candidate.startsWith("host: "));
  if (!line) throw new Error("rustc did not report a host target");
  return line.slice("host: ".length).trim();
}

function packageName(path) {
  const marker = "node_modules/";
  const normalized = path.split(sep).join("/");
  const suffix = normalized.slice(normalized.lastIndexOf(marker) + marker.length);
  const parts = suffix.split("/");
  return parts[0].startsWith("@") ? `${parts[0]}/${parts[1]}` : parts[0];
}

export function auditDeclaredLicenses(packages, ecosystem) {
  const failures = [];
  for (const dependency of packages) {
    if (!dependency.license || !String(dependency.license).trim()) {
      failures.push(`${ecosystem} dependency has no declared license: ${dependency.name}@${dependency.version}`);
    } else if (REJECTED_LICENSE.test(String(dependency.license))) {
      failures.push(
        `${ecosystem} dependency has a rejected license: ${dependency.name}@${dependency.version} (${dependency.license})`,
      );
    }
  }
  return failures;
}

function npmDependencies(root) {
  const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
  return Object.entries(lock.packages)
    .filter(([path, dependency]) => path && dependency.version)
    .map(([path, dependency]) => ({
      name: packageName(path),
      version: dependency.version,
      license: dependency.license ?? null,
      development: Boolean(dependency.dev),
    }))
    .sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`));
}

function cargoDependencies(root, target) {
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      [
        "metadata",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--format-version",
        "1",
        "--locked",
        "--offline",
        "--filter-platform",
        target,
      ],
      { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    ),
  );
  return metadata.packages
    .filter((dependency) => dependency.name !== "docflow")
    .map((dependency) => ({
      name: dependency.name,
      version: dependency.version,
      license: dependency.license,
      source: dependency.source,
    }))
    .sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`));
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function parseNativeOcrChecksumLine(line) {
  const match = line.match(/^([0-9a-fA-F]{64})\s+\.\/([^\r\n]+)\r?$/u);
  if (!match) return null;
  const relativePath = match[2];
  const segments = relativePath.split("/");
  if (
    relativePath.includes("\\") ||
    segments.some((segment) => segment === "" || segment === "." || segment === "..")
  ) {
    return null;
  }
  return { expected: match[1].toLowerCase(), relativePath };
}

function auditNativeOcr(root) {
  const resourceRoot = join(root, "src-tauri/resources/tesseract");
  const checksumPath = join(resourceRoot, "SHA256SUMS");
  const failures = [];
  if (!existsSync(checksumPath)) return ["Tesseract SHA256SUMS is missing"];
  const entries = readFileSync(checksumPath, "utf8")
    .split(/\r?\n/u)
    .filter(Boolean)
    .map((line) => {
      const entry = parseNativeOcrChecksumLine(line);
      if (!entry) {
        failures.push(`malformed Tesseract checksum line: ${line}`);
        return null;
      }
      return entry;
    })
    .filter(Boolean);
  for (const entry of entries) {
    const path = join(resourceRoot, entry.relativePath);
    if (!existsSync(path) || !statSync(path).isFile()) {
      failures.push(`Tesseract checksum target is missing: ${entry.relativePath}`);
    } else if (sha256(path) !== entry.expected) {
      failures.push(`Tesseract checksum mismatch: ${entry.relativePath}`);
    }
  }
  const evidence = [];
  const queue = [resourceRoot];
  while (queue.length > 0) {
    const directory = queue.pop();
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) queue.push(path);
      if (
        entry.isFile() &&
        (/^(LICENSE|COPYING|NOTICE)(\.|$)/iu.test(basename(path)) ||
          path.endsWith(".spdx.json"))
      ) {
        evidence.push(path);
      }
    }
  }
  if (evidence.length === 0) {
    failures.push("Tesseract dependency license evidence is missing");
  }
  for (const path of evidence) {
    if (REJECTED_LICENSE.test(relative(resourceRoot, path))) {
      failures.push(`rejected native OCR license filename: ${relative(resourceRoot, path)}`);
    }
    if (path.endsWith(".spdx.json") && REJECTED_LICENSE.test(readFileSync(path, "utf8"))) {
      failures.push(`rejected native OCR SPDX declaration: ${relative(resourceRoot, path)}`);
    }
  }
  return failures;
}

export function runAudit(root = REPOSITORY_ROOT, target = hostTarget()) {
  const npm = npmDependencies(root);
  const cargo = cargoDependencies(root, target);
  const failures = [
    ...auditDeclaredLicenses(npm, "npm"),
    ...auditDeclaredLicenses(cargo, "Cargo"),
    ...auditNativeOcr(root),
  ];
  return {
    schemaVersion: 1,
    target,
    generatedAt: new Date().toISOString(),
    summary: {
      npmPackages: npm.length,
      cargoPackages: cargo.length,
      nativeOcrChecksums: readFileSync(
        join(root, "src-tauri/resources/tesseract/SHA256SUMS"),
        "utf8",
      )
        .split(/\r?\n/u)
        .filter(Boolean).length,
      failures: failures.length,
    },
    policy: "Declared dependency licenses are required; AGPL, Affero, LicenseRef-commercial, and commercial-license matches are rejected.",
    failures,
    npm,
    cargo,
  };
}

function main(argv) {
  const targetIndex = argv.indexOf("--target");
  const reportIndex = argv.indexOf("--report");
  const target = targetIndex >= 0 ? argv[targetIndex + 1] : hostTarget();
  const reportPath = resolve(
    reportIndex >= 0
      ? argv[reportIndex + 1]
      : join(REPOSITORY_ROOT, "artifacts", `dependency-licenses-${target}.json`),
  );
  const report = runAudit(REPOSITORY_ROOT, target);
  mkdirSync(dirname(reportPath), { recursive: true });
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  if (report.failures.length > 0) {
    for (const failure of report.failures) console.error(`ERROR: ${failure}`);
    console.error(`Dependency license audit failed; report: ${reportPath}`);
    return 1;
  }
  console.log(
    `Dependency license audit: OK (${report.summary.cargoPackages} Cargo, ${report.summary.npmPackages} npm, ${report.summary.nativeOcrChecksums} OCR resource checksums)`,
  );
  console.log(reportPath);
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  process.exitCode = main(process.argv.slice(2));
}
