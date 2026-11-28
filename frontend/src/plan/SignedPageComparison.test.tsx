import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { makeStrings } from "../i18n";
import { SignedPageComparison } from "./SignedPageComparison";

it("loads an omitted signed-page comparison using only the plan and candidate identities", async () => {
  const row = {
    _rowId: "signed-row-21",
    code: "CIV-0021",
    archiveFile: "archive-choice-21",
  };
  const image = "data:image/png;base64,iVBORw0KGgo=";
  const loadComparison = vi.fn(async () => ({ archive: image, signed: image }));
  render(
    <SignedPageComparison
      plan={{
        planId: "signed-plan",
        opId: "rfi.stage_signed_first_pages",
        steps: [],
        warnings: [],
        meta: { pageComparisonOnDemand: true, pagePreviewOmitted: 1 },
      }}
      editorRows={[row]}
      focusCode=""
      s={makeStrings("en")}
      loadComparison={loadComparison}
    />,
  );

  const comparison = within(screen.getByRole("region", { name: "First-page comparison" }));
  fireEvent.click(comparison.getByRole("button", { name: "Load page comparison" }));
  expect(loadComparison).toHaveBeenCalledExactlyOnceWith(
    "signed-plan",
    "signed-row-21",
    "archive-choice-21",
  );
  expect(await comparison.findAllByRole("img")).toHaveLength(2);
});
