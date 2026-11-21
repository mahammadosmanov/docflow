import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { BusyState } from "./BusyState";
import { Button } from "./ui";

describe("BusyState progress", () => {
  afterEach(cleanup);

  it("exposes known progress as a progressbar with its values", () => {
    render(
      <BusyState
        label="Önizleme hazırlanıyor"
        progress={{ phase: "scanning", done: 3, total: 10 }}
        s={makeStrings("en")}
      />,
    );
    const bar = screen.getByRole("progressbar", { name: "Önizleme hazırlanıyor" });
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "10");
    expect(bar).toHaveAttribute("aria-valuenow", "3");
  });

  it("leaves the bar without values while the total is unknown", () => {
    render(
      <BusyState
        label="Preparing"
        progress={{ phase: "preparing", done: 0, total: 0 }}
        s={makeStrings("en")}
      />,
    );
    const bar = screen.getByRole("progressbar");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).not.toHaveAttribute("aria-valuemax");
  });

  it("announces the phase as a status", () => {
    render(
      <BusyState
        label="Preparing"
        progress={{ phase: "preparing", done: 0, total: 0 }}
        s={makeStrings("en")}
      />,
    );
    expect(screen.getByRole("status")).toBeInTheDocument();
  });
});

describe("BusyState liveness", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("animates the bar and shows elapsed time while the total is unknown", () => {
    vi.useFakeTimers();
    render(<BusyState label="Working" s={makeStrings("en")} />);
    expect(screen.getByRole("progressbar")).toHaveClass("indeterminate");
    expect(screen.queryByRole("timer")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByRole("timer")).toHaveTextContent("4s elapsed");
  });

  it("keeps a determinate bar when the total is known", () => {
    render(
      <BusyState
        label="Working"
        progress={{ phase: "scan", done: 1, total: 4 }}
        s={makeStrings("en")}
      />,
    );
    expect(screen.getByRole("progressbar")).not.toHaveClass("indeterminate");
  });
});

describe("Button busy", () => {
  afterEach(cleanup);

  it("disables the button and marks it busy", () => {
    render(<Button busy>Save</Button>);
    const button = screen.getByRole("button", { name: "Save" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
  });
});
