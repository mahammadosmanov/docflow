import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { makeStrings } from "../i18n";
import { CoverCropStrip } from "./CoverCropStrip";

const s = makeStrings("en");
const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

afterEach(cleanup);

describe("CoverCropStrip", () => {
  it("shows one small crop per row with a valid PNG", () => {
    render(
      <CoverCropStrip
        s={s}
        rows={[
          { _rowId: "a", code: "AZE-SUN-INT-QMS-RFI-PRC-0096", coverCrop: PNG },
          { _rowId: "b", code: "AZE-SUN-INT-QMS-RFI-ELE-0069", coverCrop: null },
        ]}
      />,
    );
    expect(screen.getAllByRole("img")).toHaveLength(1);
    expect(screen.getByText("AZE-SUN-INT-QMS-RFI-PRC-0096")).not.toBeNull();
  });

  it("renders nothing and rejects non-PNG sources", () => {
    const { container } = render(
      <CoverCropStrip
        s={s}
        rows={[{ _rowId: "a", code: "X", coverCrop: "https://example.test/x.png" }]}
      />,
    );
    expect(container.firstChild).toBeNull();
  });
});
