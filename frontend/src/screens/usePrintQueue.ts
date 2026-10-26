import { useEffect, useState } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type Plan, type PrintQueueSummary } from "../types";

/** Batch print queue for an operation: the latest summary plus resume and discard. */
export function usePrintQueue({
  bridge,
  operationId,
  s,
  setError,
  setStage,
  installPlan,
}: {
  bridge: Bridge;
  operationId: string;
  s: Strings;
  setError: (message: string) => void;
  setStage: (stage: "form" | "previewing") => void;
  installPlan: (plan: Plan) => void;
}) {
  const [latestPrintQueue, setLatestPrintQueue] =
    useState<PrintQueueSummary | null>(null);

  useEffect(() => {
    if (operationId !== "files.batch_print") return;
    void bridge
      .call("engine/printQueueLatest")
      .then(setLatestPrintQueue)
      .catch(() => setLatestPrintQueue(null));
  }, [bridge, operationId]);

  const resumePrintQueue = async (queueId: string) => {
    setError("");
    setStage("previewing");
    try {
      installPlan(await bridge.call("engine/printQueueResume", { queueId }));
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("form");
    }
  };
  const discardPrintQueue = async (queueId: string) => {
    setError("");
    try {
      await bridge.call("engine/printQueueDiscard", { queueId });
      setLatestPrintQueue(null);
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };

  return { latestPrintQueue, resumePrintQueue, discardPrintQueue };
}
