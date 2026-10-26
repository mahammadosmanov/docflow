import { describe, expect, it } from "vitest";

import { getOperationCategory } from "./operationCategory";

describe("getOperationCategory", () => {
  it("groups scan, Excel, mail, and system operations", () => {
    expect(getOperationCategory("rfi.process_scans_fast_covers")).toBe("scans");
    expect(getOperationCategory("rfi.system_sync")).toBe("excel");
    expect(getOperationCategory("rfi.group_files")).toBe("mail");
    expect(getOperationCategory("files.batch_print")).toBe("system");
    expect(getOperationCategory("unknown.operation")).toBe("system");
  });
});
