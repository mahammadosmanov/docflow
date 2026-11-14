import { useState } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type Plan } from "../types";

/**
 * Cancel actions for a running preview or apply. `cancelling` guards against a second request.
 * With `twoStageCancel`, the first apply request only pauses the queue after the current file
 * (`pausing`); the second request cancels the active job.
 */
export function useRunCancel({
  bridge,
  operationId,
  plan,
  s,
  setError,
  onPreviewCancelled,
  twoStageCancel,
}: {
  bridge: Bridge;
  operationId: string;
  plan: Plan | null;
  s: Strings;
  setError: (message: string) => void;
  onPreviewCancelled: () => void;
  twoStageCancel: boolean;
}) {
  const [cancelling, setCancelling] = useState(false);
  const [pausing, setPausing] = useState(false);

  const cancelPreview = async () => {
    if (cancelling) return;
    setCancelling(true);
    try {
      await bridge.call("engine/cancel", { opId: operationId });
    } catch {
      // Ignored if preview already completed or errored
    } finally {
      setCancelling(false);
      onPreviewCancelled();
    }
  };

  const cancelApply = async () => {
    if (!plan || cancelling) return;
    if (twoStageCancel && !pausing) {
      try {
        await bridge.call("engine/cancel", { planId: plan.planId });
        setPausing(true);
      } catch (reason) {
        setError(humanError(reason, s));
      }
      return;
    }
    setCancelling(true);
    try {
      await bridge.call("engine/cancel", { planId: plan.planId });
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };

  return { cancelling, setCancelling, pausing, setPausing, cancelPreview, cancelApply };
}
