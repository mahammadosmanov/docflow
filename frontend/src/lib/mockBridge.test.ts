import { describe, expect, it } from "vitest";

import { MockBridge } from "./mockBridge";

type PrintFindResult = {
  candidates: { rfiCode: string; documentType: string; selected: boolean }[];
  resolutions: { requested: string; rfiCodes: string[]; missingFirstPage: boolean }[];
};

describe("MockBridge rfi/printFind", () => {
  it("resolves a full RFI number to its first-page candidate", async () => {
    const result = (await new MockBridge().call("rfi/printFind", {
      numbers: ["mec-0012"],
      firstPage: true,
      sketch: false,
    } as never)) as unknown as PrintFindResult;

    expect(result.candidates).toHaveLength(1);
    expect(result.candidates[0]).toMatchObject({
      rfiCode: "AZE-SUN-INT-QMS-RFI-MEC-0012",
      documentType: "firstPage",
      selected: true,
    });
    expect(result.resolutions[0]).toMatchObject({
      requested: "mec-0012",
      rfiCodes: ["AZE-SUN-INT-QMS-RFI-MEC-0012"],
      missingFirstPage: false,
    });
  });
});
