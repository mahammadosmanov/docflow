import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { Modal } from "./ui";

describe("Modal", () => {
  it("names the dialog after its title", () => {
    render(
      <Modal title="Undo operation" closeLabel="Close" onClose={vi.fn()}>
        <p>body</p>
      </Modal>,
    );
    expect(screen.getByRole("dialog", { name: "Undo operation" })).toBeInTheDocument();
  });

  it("labels the close button with the translated text it is given", () => {
    render(
      <Modal title="Preview" closeLabel="Kapat" onClose={vi.fn()}>
        <p>body</p>
      </Modal>,
    );
    expect(screen.getByRole("button", { name: "Kapat" })).toBeInTheDocument();
  });
});
