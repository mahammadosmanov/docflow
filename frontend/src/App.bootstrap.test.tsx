import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { failingBridge } = vi.hoisted(() => ({
  failingBridge: {
    start: vi.fn(async () => {}),
    call: vi.fn(async () => {
      throw new Error("bootstrap unavailable");
    }),
    stop: vi.fn(async () => {}),
  },
}));

vi.mock("./lib/bridge", () => ({
  getBridge: () => failingBridge,
}));

import { App } from "./App";

beforeEach(() => {
  Object.defineProperty(window.navigator, "language", {
    configurable: true,
    value: "ru-RU",
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("bootstrap failure", () => {
  it("uses the safe browser-supported locale before preferences load", async () => {
    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "Не удалось запустить DocFlow",
      }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Повторить" })).toBeVisible();
    expect(screen.getByText("bootstrap unavailable")).toBeVisible();
  });
});
