import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

const dirs = ["frontend/src/styles", "frontend/src/components"];
const cssFiles = dirs.flatMap((dir) =>
  readdirSync(resolve(process.cwd(), dir))
    .filter((name) => name.endsWith(".css"))
    .map((name) => `${dir}/${name}`),
);

describe("type scale floor", () => {
  it("has no font-size below 12px", () => {
    const offender = /font-size:\s*(0\.[0-6][0-9]*rem|[0-9](\.[0-9]+)?px|1[01](\.[0-9]+)?px)/;
    const hits = cssFiles.flatMap((file) =>
      readFileSync(resolve(process.cwd(), file), "utf8")
        .split("\n")
        .filter((line) => offender.test(line))
        .map((line) => `${file}: ${line.trim()}`),
    );
    expect(hits).toEqual([]);
  });
});
