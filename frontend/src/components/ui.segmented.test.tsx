import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Segmented } from "./ui";

describe("Segmented", () => {
  afterEach(cleanup);

  it("exposes the chosen option as pressed and the others as not pressed", () => {
    render(
      <Segmented
        value="put"
        options={[
          ["keep", "Bırak"],
          ["put", "Taşı"],
        ]}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Taşı" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Bırak" })).toHaveAttribute("aria-pressed", "false");
  });

  it("names its group so repeated controls can be told apart", () => {
    render(
      <Segmented
        label="rapor.pdf"
        value="keep"
        options={[["keep", "Bırak"]]}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByRole("group", { name: "rapor.pdf" })).toBeInTheDocument();
  });

  it("reports the selected option when clicked", () => {
    const onChange = vi.fn();
    render(
      <Segmented
        value="keep"
        options={[
          ["keep", "Bırak"],
          ["delete", "Sil"],
        ]}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Sil" }));
    expect(onChange).toHaveBeenCalledWith("delete");
  });
});
