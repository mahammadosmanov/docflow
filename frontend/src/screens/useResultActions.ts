import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type OperationResult } from "../types";

/** Actions on a finished run: undo it. */
export function useResultActions({
  bridge,
  result,
  s,
  setError,
  onBack,
}: {
  bridge: Bridge;
  result: OperationResult | null;
  s: Strings;
  setError: (message: string) => void;
  onBack: () => void;
}) {
  const undo = async () => {
    if (!result?.journalId) return;
    try {
      await bridge.call("engine/undo", {
        journalId: result.journalId,
      });
      onBack();
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };

  return { undo };
}
