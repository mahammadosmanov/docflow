import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { makeStrings } from "../i18n";
import { BusyState } from "./ui";

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
