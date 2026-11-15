import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

const dirs = ["frontend/src/styles", "frontend/src/components"];
const cssFiles = dirs.flatMap((dir) =>
  readdirSync(resolve(process.cwd(), dir))
    .filter((name) => name.endsWith(".css"))
    .map((name) => `${dir}/${name}`),
);
const source = (file: string) => readFileSync(resolve(process.cwd(), file), "utf8");
const hitsFor = (pattern: RegExp) =>
  cssFiles.flatMap((file) =>
    source(file)
      .split("\n")
      .filter((line) => pattern.test(line))
      .map((line) => `${file}: ${line.trim()}`),
  );

describe("type scale floor", () => {
  it("has no font-size below 12px", () => {
    const offender = /font-size:\s*(0\.([0-6][0-9]*|7[0-4]?)rem|[0-9](\.[0-9]+)?px|1[01](\.[0-9]+)?px)/;
    expect(hitsFor(offender)).toEqual([]);
  });
});

describe("style tokens", () => {
  it("uses the radius scale instead of 4-14px literals", () => {
    expect(hitsFor(/border-radius:\s*([4-9]|1[0-4])px\s*;/)).toEqual([]);
  });

  it("uses the focus ring token instead of a literal outline", () => {
    expect(hitsFor(/outline:\s*2px solid var\(--text\)/)).toEqual([]);
  });
});
