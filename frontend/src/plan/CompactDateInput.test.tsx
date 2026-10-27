import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import { CompactDateInput } from "./CompactDateInput";

describe("CompactDateInput", () => {
  afterEach(cleanup);

  it("clamps the day to the new month's length when the month changes", () => {
    const onChange = vi.fn();
    render(
      <CompactDateInput
        value="31-Jan-24"
        label="Date"
        s={makeStrings("en")}
        onChange={onChange}
      />,
    );
    const [, month] = screen.getAllByRole("combobox");
    fireEvent.change(month, { target: { value: "2" } });
    expect(onChange).toHaveBeenCalledWith("29-Feb-24");
  });

  it("waits until both day and month are chosen before reporting a value", () => {
    const onChange = vi.fn();
    render(
      <CompactDateInput
        value=""
        label="Date"
        s={makeStrings("en")}
        onChange={onChange}
      />,
    );
    const [day] = screen.getAllByRole("combobox");
    fireEvent.change(day, { target: { value: "5" } });
    expect(onChange).not.toHaveBeenCalled();
  });
});
