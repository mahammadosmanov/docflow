import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const catalog = (locale: string): Record<string, string> =>
  JSON.parse(readFileSync(resolve(process.cwd(), `frontend/src/locales/app_${locale}.arb`), "utf8"));
const messageKeys = (entries: Record<string, string>) =>
  Object.keys(entries).filter((key) => !key.startsWith("@")).sort();
const placeholders = (text: string) =>
  [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

const en = catalog("en");
const others = ["az", "ru", "tr"];

const sourceFiles = (dir: string): string[] =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });

describe("ARB catalogs", () => {
  it("defines every literal s.t() key the frontend uses", () => {
    const used = new Set(
      sourceFiles(resolve(process.cwd(), "frontend/src")).flatMap((file) =>
        [...readFileSync(file, "utf8").matchAll(/\bs\.t\(\s*"([^"]+)"/g)].map(
          (match) => match[1],
        ),
      ),
    );
    const missing = [...used].filter((key) => !(key in en)).sort();
    expect(missing).toEqual([]);
  });

  for (const locale of others) {
    const entries = catalog(locale);

    it(`${locale} has exactly the English message keys`, () => {
      expect(messageKeys(entries)).toEqual(messageKeys(en));
    });

    it(`${locale} uses the same placeholders as English for every message`, () => {
      const mismatched = messageKeys(en).filter(
        (key) =>
          JSON.stringify(placeholders(String(entries[key]))) !==
          JSON.stringify(placeholders(en[key])),
      );
      expect(mismatched).toEqual([]);
    });
  }
});
