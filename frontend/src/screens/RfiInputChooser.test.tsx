import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { RfiInputChooser } from "./RfiInputChooser";

const handlers = () => ({
  onSelectAll: vi.fn(),
  onSelectNone: vi.fn(),
  onToggle: vi.fn(),
});

describe("RfiInputChooser", () => {
  afterEach(cleanup);

  it("renders nothing when no RFI numbers were typed", () => {
    const { container } = render(
      <RfiInputChooser inputs={[]} excluded={[]} s={makeStrings("en")} {...handlers()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("checks every number that is not excluded", () => {
    render(
      <RfiInputChooser
        inputs={["RFI-1", "RFI-2"]}
        excluded={["RFI-2"]}
        s={makeStrings("en")}
        {...handlers()}
      />,
    );
    expect(screen.getByRole("checkbox", { name: "RFI-1" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "RFI-2" })).not.toBeChecked();
  });

  it("reports the new checked state of the number that was toggled", () => {
    const h = handlers();
    render(
      <RfiInputChooser
        inputs={["RFI-1", "RFI-2"]}
        excluded={["RFI-2"]}
        s={makeStrings("en")}
        {...h}
      />,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "RFI-2" }));
    expect(h.onToggle).toHaveBeenCalledWith("RFI-2", true);
  });

  it("forwards the select-all and select-none actions", () => {
    const h = handlers();
    render(
      <RfiInputChooser inputs={["RFI-1"]} excluded={[]} s={makeStrings("en")} {...h} />,
    );
    fireEvent.click(screen.getByRole("button", { name: makeStrings("en").t("action.selectAll") }));
    fireEvent.click(screen.getByRole("button", { name: makeStrings("en").t("action.selectNone") }));
    expect(h.onSelectAll).toHaveBeenCalledTimes(1);
    expect(h.onSelectNone).toHaveBeenCalledTimes(1);
  });
});
