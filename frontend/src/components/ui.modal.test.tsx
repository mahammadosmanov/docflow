import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Modal } from "./ui";

describe("Modal", () => {
  afterEach(cleanup);

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

  it("moves focus into the dialog and returns it to the opener on close", () => {
    const opener = document.createElement("button");
    document.body.append(opener);
    opener.focus();
    const { unmount } = render(
      <Modal title="Preview" closeLabel="Kapat" onClose={vi.fn()}>
        <button type="button">Kaydet</button>
      </Modal>,
    );
    expect(screen.getByRole("dialog")).toHaveFocus();
    unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });

  it("keeps Tab focus inside the dialog", () => {
    render(
      <Modal title="Preview" closeLabel="Kapat" onClose={vi.fn()}>
        <button type="button">Kaydet</button>
      </Modal>,
    );
    const close = screen.getByRole("button", { name: "Kapat" });
    const save = screen.getByRole("button", { name: "Kaydet" });
    save.focus();
    fireEvent.keyDown(save, { key: "Tab" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(save).toHaveFocus();
  });
});
