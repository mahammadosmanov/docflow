import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { Strings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import { FollowUpExceptionsSettings } from "./FollowUpExceptionsSettings";

test("loading exceptions is read-only and each edit sends one code", async () => {
  const call = vi.fn().mockImplementation(async (method) =>
    method === "settings/followUpExceptions"
      ? { shared: ["CIV-0099"], local: ["MEC-0346"] }
      : { ok: true },
  );
  const bridge = { call } as unknown as Bridge;
  const s = { t: (key: string) => key } as Strings;
  render(<FollowUpExceptionsSettings bridge={bridge} s={s} />);

  fireEvent.click(screen.getByRole("button", { name: "prefs.followUpExceptions.load" }));
  expect(await screen.findByLabelText("prefs.followUpExceptions.shared")).toHaveValue("CIV-0099");
  expect(screen.getByLabelText("prefs.followUpExceptions.local")).toHaveValue("MEC-0346");
  expect(call.mock.calls).toEqual([["settings/followUpExceptions"]]);

  fireEvent.change(screen.getByLabelText("prefs.followUpExceptions.code"), {
    target: { value: "MEC-0346" },
  });
  fireEvent.click(screen.getByRole("button", { name: "prefs.followUpExceptions.removeLocal" }));
  await screen.findByText("prefs.followUpExceptions.saved");
  expect(call).toHaveBeenCalledWith("settings/changeFollowUpException", {
    code: "MEC-0346",
    change: "removeLocal",
  });
});
