import { describe, expect, it } from "vitest";
import {
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { App } from "./App";
import { signIn, registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: home screen filters", () => {
  it("filters operations by search text and category pills on home screen", async () => {
    render(<App />);
    await signIn();

    const searchInput = screen.getByPlaceholderText("İşlem ara…");
    expect(searchInput).toBeVisible();

    fireEvent.change(searchInput, { target: { value: "Yazdır" } });
    expect(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: "" } });
    expect(
      screen.getByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).toBeVisible();

    const chipsContainer = document.querySelector(".category-filter-chips")!;
    const scansChip = within(chipsContainer as HTMLElement).getByRole("button", {
      name: /Taramalar ve PDF/,
    });
    fireEvent.click(scansChip);
    expect(scansChip).toHaveClass("active");
    expect(
      screen.getByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).not.toBeInTheDocument();
  });
});
