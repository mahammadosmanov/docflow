import { describe, expect, it } from "vitest";

import { withDisciplinePrefix } from "./templateDiscipline";

describe("withDisciplinePrefix", () => {
  it("adds, swaps, and keeps the rest of the text", () => {
    expect(withDisciplinePrefix("", "ELE")).toBe("ELE-");
    expect(withDisciplinePrefix("RFI-COVER", "ELE")).toBe("ELE-RFI-COVER");
    expect(withDisciplinePrefix("ELE-RFI Cover", "MEC")).toBe("MEC-RFI Cover");
    expect(withDisciplinePrefix("civ-x", "PRC")).toBe("PRC-x");
  });
});
