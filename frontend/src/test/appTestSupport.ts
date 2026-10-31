import {
  cleanup,
  fireEvent,
  screen,
} from "@testing-library/react";
import { afterEach, expect, vi } from "vitest";

import { MockBridge, resetBridgeForTests } from "../lib/bridge";

export function interceptMockBridge(
  intercept: (
    method: string,
    params: Record<string, unknown>,
  ) => { handled: boolean; value?: unknown },
) {
  const originalCall = MockBridge.prototype.call;
  return vi.spyOn(MockBridge.prototype, "call").mockImplementation(function (
    this: MockBridge,
    method,
    ...args
  ) {
    const result = intercept(
      method,
      (args[0] ?? {}) as Record<string, unknown>,
    );
    if (result.handled) return Promise.resolve(result.value) as never;
    return Reflect.apply(originalCall, this, [method, ...args]);
  });
}

export async function signIn() {
  expect(await screen.findByRole("heading", { name: "DocFlow" })).toBeVisible();
  fireEvent.change(screen.getByLabelText("E-posta"), {
    target: { value: "demo@example.com" },
  });
  fireEvent.change(screen.getByLabelText("Parola"), {
    target: { value: "secret1" },
  });
  const submit = document.querySelector<HTMLButtonElement>(
    'button[type="submit"]',
  );
  expect(submit).not.toBeNull();
  fireEvent.click(submit!);
  await screen.findByRole("heading", { name: "Çalışma alanı" });
}

export function registerAppTestCleanup() {
  afterEach(() => {
    cleanup();
    resetBridgeForTests();
    vi.restoreAllMocks();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-font");
    document.documentElement.removeAttribute("data-compact");
  });
}
