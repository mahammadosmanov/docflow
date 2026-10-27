import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useOperationShortcuts } from "./useOperationShortcuts";

afterEach(cleanup);

function setup(overrides: Partial<Parameters<typeof useOperationShortcuts>[0]> = {}) {
  const handlers = {
    stage: "form" as const,
    cancelling: false,
    preview: vi.fn(),
    apply: vi.fn(),
    cancelPreview: vi.fn(),
    setStage: vi.fn(),
    onBack: vi.fn(),
    ...overrides,
  };
  renderHook(() => useOperationShortcuts(handlers));
  return handlers;
}

const press = (key: string, init: KeyboardEventInit = {}) =>
  window.dispatchEvent(new KeyboardEvent("keydown", { key, ...init }));

describe("useOperationShortcuts", () => {
  it("previews from the form on Ctrl+Enter and applies from the preview", () => {
    const form = setup();
    press("Enter", { ctrlKey: true });
    expect(form.preview).toHaveBeenCalledTimes(1);
    expect(form.apply).not.toHaveBeenCalled();
  });

  it("applies from the preview on Cmd+Enter but not while cancelling", () => {
    const preview = setup({ stage: "preview" });
    press("Enter", { metaKey: true });
    expect(preview.apply).toHaveBeenCalledTimes(1);
    cleanup();
    const cancelling = setup({ stage: "preview", cancelling: true });
    press("Enter", { ctrlKey: true });
    expect(cancelling.apply).not.toHaveBeenCalled();
  });

  it("Escape cancels a running preview", () => {
    const running = setup({ stage: "previewing" });
    press("Escape");
    expect(running.cancelPreview).toHaveBeenCalledTimes(1);
  });

  it("Escape returns from preview or result to the form", () => {
    const result = setup({ stage: "result" });
    press("Escape");
    expect(result.setStage).toHaveBeenCalledWith("form");
  });

  it("Escape on the form leaves the screen", () => {
    const form = setup();
    press("Escape");
    expect(form.onBack).toHaveBeenCalledTimes(1);
  });
});
