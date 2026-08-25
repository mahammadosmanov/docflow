import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(new URL("..", import.meta.url).pathname);

test("Windows installer smoke launches the installed GUI before package verification", () => {
  const source = readFileSync(resolve(root, "tools/smoke_windows_installer.ps1"), "utf8");

  assert.match(source, /Start-Process\s+-FilePath\s+\$application\s+-PassThru/u);
  assert.match(source, /if\s*\(\$gui\.HasExited\)/u);
  assert.match(source, /Stop-Process\s+-Id\s+\$gui\.Id\s+-Force/u);
});
