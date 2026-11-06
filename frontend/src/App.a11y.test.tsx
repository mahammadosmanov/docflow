import axe from "axe-core";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { signIn, registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

async function violations() {
  const result = await axe.run(document.body, {
    // jsdom has no layout or computed colors; contrast is checked natively.
    rules: { "color-contrast": { enabled: false } },
  });
  return result.violations.map(
    (violation) =>
      `${violation.id}: ${violation.nodes.map((node) => node.target.join(" ")).join(", ")}`,
  );
}

describe("DocFlow accessibility", () => {
  it("renders the sign-in screen without axe violations", async () => {
    render(<App />);
    await screen.findByLabelText("E-posta");
    expect(await violations()).toEqual([]);
  });

  it("renders home and an operation form without axe violations", async () => {
    render(<App />);
    await signIn();
    expect(await violations()).toEqual([]);

    fireEvent.click(document.querySelector(".primary-scan-card")!);
    await screen.findByRole("heading", { name: /Yapılandır/ });
    expect(await violations()).toEqual([]);
  });
});
