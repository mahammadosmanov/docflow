import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { BusyStage } from "./BusyStage";

const base = {
  label: "Önizleme hazırlanıyor",
  s: makeStrings("en"),
  cancelLabel: "Cancel",
  progressTrail: [],
};

describe("BusyStage", () => {
  afterEach(cleanup);

  it("calls onCancel when the cancel button is pressed", () => {
    const onCancel = vi.fn();
    render(<BusyStage {...base} cancelling={false} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("disables cancel while a cancellation is already underway", () => {
    render(<BusyStage {...base} cancelling onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("names the running operation with its label", () => {
    render(<BusyStage {...base} cancelling={false} onCancel={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Önizleme hazırlanıyor" })).toBeInTheDocument();
  });
});
