import { describe, expect, it } from "vitest";

import { makeStrings } from "../i18n";
import type { WatchItem } from "../types";
import { failureMessage } from "./QueueScreen";

const s = makeStrings("en");
const item = (errorCode?: string) => ({ errorCode }) as WatchItem;

describe("failureMessage", () => {
  it("uses the watch-specific text for a known watch error code", () => {
    expect(failureMessage(item("watch.notReady"), s)).toBe(s.t("error.watch.notReady"));
  });

  it("falls back to the generic text, naming the code", () => {
    const message = failureMessage(item("engine.boom"), s);
    expect(message).toBe(s.t("queue.failed.generic", { code: "engine.boom" }));
    expect(message).toContain("engine.boom");
  });

  it("falls back to the generic text when there is no code", () => {
    expect(failureMessage(item(undefined), s)).toBe(s.t("queue.failed.generic", { code: "" }));
  });
});
