import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import { UpdateNotice } from "./UpdateNotice";

describe("UpdateNotice", () => {
  it("shows a checked update and installs it only after the user clicks", async () => {
    const call = vi.fn(async (method: string) =>
      method === "update/check"
        ? { configured: true, available: true, version: "1.38.0", notes: "Safer updates" }
        : { ok: true },
    );
    render(<UpdateNotice bridge={{ call } as unknown as Bridge} s={makeStrings("en")} />);

    expect(await screen.findByText("DocFlow 1.38.0 is available")).toBeTruthy();
    expect(screen.getByText("Safer updates")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("update/install", undefined, expect.any(Function)));
  });

  it("lets the user check again after an installation failure", async () => {
    const call = vi.fn(async (method: string) => {
      if (method === "update/check") {
        return { configured: true, available: true, version: "1.38.0", notes: "Release" };
      }
      throw new Error("installer failed");
    });
    render(<UpdateNotice bridge={{ call } as unknown as Bridge} s={makeStrings("en")} />);

    fireEvent.click(await screen.findByRole("button", { name: "Install update" }));
    expect(await screen.findByText(/Could not check or install the update/u)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await waitFor(() => expect(call).toHaveBeenCalledTimes(3));
  });
});
