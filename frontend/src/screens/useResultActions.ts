import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type OperationResult, type Plan } from "../types";

/** Actions on a finished run: prepare an RFI reply from it, or undo it. */
export function useResultActions({
  bridge,
  plan,
  result,
  s,
  setError,
  onBack,
  onPrepareRfiReply,
}: {
  bridge: Bridge;
  plan: Plan | null;
  result: OperationResult | null;
  s: Strings;
  setError: (message: string) => void;
  onBack: () => void;
  onPrepareRfiReply: (plan: Plan) => void;
}) {
  const prepareRfiReply = async () => {
    if (!plan || !result?.journalId) return;
    setError("");
    try {
      onPrepareRfiReply(
        await bridge.call("engine/rfiReplyPreview", {
          planId: plan.planId,
          journalId: result.journalId,
        }),
      );
    } catch (reason) {
      setError(humanError(reason, s));
    }
  };

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

  return { prepareRfiReply, undo };
}
