import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { PathSelection, ReviewItem } from "../types";
import { ReviewScreen } from "./ReviewScreen";

const s = makeStrings("en");
const destination = { selectionId: "dest", name: "out", displayPath: "/out" } as PathSelection;
const item: ReviewItem = {
  selectionId: "file-1",
  path: "/in/a.pdf",
  name: "a.pdf",
  code: "A-1",
  confidence: 0.9,
  targetExists: true,
};

describe("ReviewScreen", () => {
  afterEach(cleanup);

  it("warns that a move replaces an existing destination file", async () => {
    const bridge = {
      call: vi.fn().mockResolvedValue([item]),
    } as unknown as Bridge;
    render(
      <ReviewScreen bridge={bridge} destination={destination} s={s} onBack={vi.fn()} />,
    );
    const put = await screen.findByRole("button", { name: s.t("review.put") });
    expect(screen.queryByText(new RegExp(s.t("review.willReplace")))).toBeNull();
    fireEvent.click(put);
    await waitFor(() =>
      expect(screen.getByText(new RegExp(s.t("review.willReplace")))).toBeTruthy(),
    );
  });

  it("shows the error and keeps the result when undo fails", async () => {
    const plan = {
      planId: "p1",
      opId: "rfi.review",
      steps: [{ kind: "move", src: "/in/a.pdf", dst: "/out/a.pdf", noteKey: null, meta: {} }],
      warnings: [],
      meta: {},
    };
    const result = { applied: 1, failed: 0, journalId: "j1", errors: [], meta: {} };
    const call = vi.fn(async (name: string) => {
      if (name === "engine/reviewScan") return [item];
      if (name === "engine/reviewPreview") return plan;
      if (name === "engine/apply") return result;
      if (name === "engine/undo") throw new Error("undo blocked");
      return undefined;
    });
    const bridge = { call } as unknown as Bridge;
    render(
      <ReviewScreen bridge={bridge} destination={destination} s={s} onBack={vi.fn()} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: s.t("review.put") }));
    fireEvent.click(await screen.findByRole("button", { name: s.t("action.preview") }));
    fireEvent.click(await screen.findByRole("button", { name: s.t("action.apply") }));
    fireEvent.click(await screen.findByRole("button", { name: s.t("action.undo") }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("engine/undo", { journalId: "j1" }));
    expect(await screen.findByText(/undo blocked/)).toBeTruthy();
    expect(screen.getByRole("button", { name: s.t("action.undo") })).toBeTruthy();
  });
});
