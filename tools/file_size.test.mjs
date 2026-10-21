import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const MAX_LINES = 2500;
const roots = ["src-tauri/src", "frontend/src"];
const isTest = (name) =>
  /(^|_)tests?\.(rs|tsx?)$/.test(name) || /\.test\.tsx?$/.test(name);

function* sources(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* sources(path);
    else if (/\.(rs|tsx?)$/.test(entry.name) && !isTest(entry.name)) yield path;
  }
}

test(`production source files stay under ${MAX_LINES} lines`, () => {
  const oversized = roots
    .flatMap((root) => [...sources(root)])
    .map((path) => [path, readFileSync(path, "utf8").split("\n").length])
    .filter(([, lines]) => lines > MAX_LINES);
  assert.deepEqual(oversized, []);
});
