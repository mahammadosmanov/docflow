import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { loadLocale } from "../i18n";
import { ErrorBoundary } from "./ErrorBoundary";

function Crash(): never {
  throw new Error("render failed");
}

describe("ErrorBoundary", () => {
  beforeEach(async () => {
    await loadLocale("en");
    vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders its children when nothing fails", () => {
    render(
      <ErrorBoundary>
        <p>ok</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("ok")).toBeInTheDocument();
  });

  it("shows an alert with a reload button instead of a blank screen", () => {
    render(
      <ErrorBoundary>
        <Crash />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(/unexpected problem/);
    expect(screen.getByRole("button", { name: "Reload" })).toBeInTheDocument();
  });
});
