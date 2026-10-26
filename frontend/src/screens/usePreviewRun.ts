import { type MutableRefObject } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { persistableValues, previewValuesFor } from "../lib/operationValues";
import { type OperationDef, type Plan, type ProgressEvent } from "../types";
import { type RunStage } from "./useApplyRun";

/** Saves the parameters, then asks the engine for a preview plan. */
export function usePreviewRun({
  bridge,
  operation,
  values,
  lastSaved,
  finder,
  s,
  screen,
  progress,
}: {
  bridge: Bridge;
  operation: Pick<OperationDef, "id" | "schema" | "supportsPresets">;
  values: Record<string, unknown>;
  lastSaved: MutableRefObject<string>;
  finder: Parameters<typeof previewValuesFor>[2];
  s: Strings;
  screen: {
    setError: (message: string) => void;
    setStage: (stage: RunStage) => void;
    setPreviousValues: (values: Record<string, unknown>) => void;
    installPlan: (plan: Plan) => void;
  };
  progress: {
    startProgress: (event: ProgressEvent) => void;
    receiveProgress: (event: ProgressEvent) => void;
  };
}) {
  const { setError, setStage, setPreviousValues, installPlan } = screen;

  const preview = async () => {
    setError("");
    const preparing = { phase: "preparing", done: 0, total: 0 };
    progress.startProgress(preparing);
    setStage("previewing");
    try {
      const sanitized = persistableValues(operation.schema, values);
      lastSaved.current = JSON.stringify(sanitized);
      await bridge.call("settings/saveParams", {
        opId: operation.id,
        values: sanitized,
      });
      if (operation.supportsPresets && Object.keys(sanitized).length > 0) {
        setPreviousValues(sanitized);
      }
      const previewValues = previewValuesFor(operation.id, values, finder);
      const next = await bridge.call(
        "engine/preview",
        { opId: operation.id, values: previewValues },
        progress.receiveProgress,
      );
      installPlan(next);
    } catch (reason) {
      setError(humanError(reason, s));
      setStage("form");
    }
  };

  return { preview };
}
