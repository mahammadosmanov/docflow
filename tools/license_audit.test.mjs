import assert from "node:assert/strict";
import test from "node:test";

import { auditDeclaredLicenses } from "./license_audit.mjs";

test("license audit rejects missing, Affero, and commercial declarations", () => {
  const failures = auditDeclaredLicenses(
    [
      { name: "missing", version: "1.0.0", license: null },
      { name: "affero", version: "1.0.0", license: "AGPL-3.0" },
      { name: "commercial", version: "1.0.0", license: "LicenseRef-commercial" },
      { name: "allowed", version: "1.0.0", license: "MIT OR Apache-2.0" },
    ],
    "fixture",
  );

  assert.equal(failures.length, 3);
  assert.match(failures.join("\n"), /missing/);
  assert.match(failures.join("\n"), /affero/);
  assert.match(failures.join("\n"), /commercial/);
  assert.doesNotMatch(failures.join("\n"), /allowed/);
});
