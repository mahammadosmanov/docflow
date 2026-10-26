import { useState } from "react";

import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type Plan } from "../types";

/** Re-runs the fast-scan plan after its editor rows were changed. */
export function useScanPreviewUpdate({
  bridge,
  plan,
  editorDirty,
  editorRows,
  s,
  setError,
  onRevised,
}: {
  bridge: Bridge;
  plan: Plan | null;
  editorDirty: boolean;
  editorRows: Record<string, unknown>[];
  s: Strings;
  setError: (message: string) => void;
  onRevised: (revised: Plan) => void;
}) {
  const [isUpdatingScanPreview, setIsUpdatingScanPreview] = useState(false);

  const updateScanPreview = async () => {
    if (
      !plan ||
      plan.opId !== "rfi.process_scans_fast_covers" ||
      !editorDirty ||
      isUpdatingScanPreview
    ) {
      return;
    }
    setIsUpdatingScanPreview(true);
    setError("");
    try {
      const revised = await bridge.call("engine/revise", {
        planId: plan.planId,
        rows: editorRows,
      });
      onRevised(revised);
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setIsUpdatingScanPreview(false);
    }
  };

  return { isUpdatingScanPreview, updateScanPreview };
}
