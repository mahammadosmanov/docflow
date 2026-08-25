import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { FRONTEND_METHODS } from "./commands";

const repositoryRoot = process.cwd();
const read = (path: string) => readFileSync(resolve(repositoryRoot, path), "utf8");

describe("frontend authority boundary", () => {
  it("has no shell, process, filesystem, network, opener, or credential authority", () => {
    const packageJson = JSON.parse(read("package.json")) as {
      dependencies: Record<string, string>;
    };
    const capability = JSON.parse(
      read("src-tauri/capabilities/default.json"),
    ) as { permissions: string[] };
    const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json")) as {
      app: { security: { csp: string } };
    };
    const sources = [
      "frontend/src/App.tsx",
      "frontend/src/lib/bridge.ts",
      "frontend/src/lib/commands.ts",
      "frontend/src/lib/platform.ts",
      "frontend/src/main.tsx",
    ]
      .map(read)
      .join("\n");

    expect(repositoryRoot).toContain("docflow");
    expect(Object.keys(packageJson.dependencies)).not.toContain(
      "@tauri-apps/plugin-dialog",
    );
    expect(Object.keys(packageJson.dependencies)).not.toContain(
      "@tauri-apps/plugin-opener",
    );
    expect(capability.permissions).toEqual(["core:default"]);
    expect(tauriConfig.app.security.csp).toContain(
      "connect-src ipc: http://ipc.localhost",
    );
    expect(tauriConfig.app.security.csp).not.toMatch(
      /connect-src[^;]*https?:\/\/(?!ipc\.localhost)/,
    );
    expect(sources).not.toMatch(
      /@tauri-apps\/plugin-(?:dialog|fs|http|opener|shell)|child_process|localStorage|sessionStorage|\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource|accessToken|refreshToken|SUPABASE/i,
    );
    expect(
      [...sources.matchAll(/\binvoke[^\n(]*\(\s*["']([^"']+)["']/g)].map(
        (match) => match[1],
      ),
    ).toEqual(["frontend_call"]);
  });

  it("keeps TypeScript usage equal to the closed command roster", () => {
    const usage = ["frontend/src/App.tsx", "frontend/src/lib/platform.ts"]
      .map(read)
      .join("\n");
    const used = [...usage.matchAll(/\.call\(\s*["']([^"']+)["']/g)]
      .map((match) => match[1])
      .sort();
    const roster = [...FRONTEND_METHODS].sort();

    expect([...new Set(used)]).toEqual(roster);
    expect(usage).not.toMatch(/\.call\s*</);
    expect(read("frontend/src/lib/bridge.ts")).not.toContain("method: string");
  });

  it("keeps every user-facing locale key available in all four catalogs", () => {
    const locales = ["tr", "en", "ru", "az"].map((locale) =>
      Object.keys(
        JSON.parse(
          read(`frontend/src/locales/app_${locale}.arb`),
        ) as Record<string, unknown>,
      )
        .filter((key) => !key.startsWith("@"))
        .sort(),
    );

    for (const keys of locales.slice(1)) expect(keys).toEqual(locales[0]);
  });

  it("defines localized bootstrap failure title and retry text in every catalog", () => {
    for (const locale of ["tr", "en", "ru", "az"]) {
      const catalog = JSON.parse(
        read(`frontend/src/locales/app_${locale}.arb`),
      ) as Record<string, unknown>;
      expect(catalog["bootstrap.fatalTitle"]).toEqual(expect.any(String));
      expect(catalog["bootstrap.retry"]).toEqual(expect.any(String));
    }
  });
});
