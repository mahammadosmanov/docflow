import { useState } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type Plan } from "../types";

/** Copies the plan's table, or one sheet of it, and reports how many rows were copied. */
export function useCopyTable({
  bridge,
  plan,
  s,
  setError,
}: {
  bridge: Bridge;
  plan: Plan | null;
  s: Strings;
  setError: (message: string) => void;
}) {
  const [copyingSheet, setCopyingSheet] = useState<string | null>(null);

  const copyTable = async (sheet?: string) => {
    if (!plan || copyingSheet !== null) return;
    setCopyingSheet(sheet ?? "");
    setError("");
    try {
      const payload = await bridge.call("engine/copyTable", {
        planId: plan.planId,
        sheet,
      });
      setError(s.t("rfi.editor.copied", { count: payload.count }));
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setCopyingSheet(null);
    }
  };

  return { copyingSheet, copyTable };
}
