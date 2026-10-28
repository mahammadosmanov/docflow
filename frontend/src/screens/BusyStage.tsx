import { LoaderCircle, X } from "lucide-react";
import { type ComponentProps } from "react";

import { ActionBar, Button } from "../components/ui";
import { BusyState } from "../components/BusyState";

type BusyStageProps = Pick<
  ComponentProps<typeof BusyState>,
  "label" | "progress" | "progressTrail" | "s"
> & {
  cancelling: boolean;
  cancelLabel: string;
  onCancel: () => void;
};

/** Progress for a running preview or apply, with the one cancel action. */
export function BusyStage({
  label,
  progress,
  progressTrail,
  s,
  cancelling,
  cancelLabel,
  onCancel,
}: BusyStageProps) {
  return (
    <div className="flow-layout">
      <BusyState
        label={label}
        progress={progress}
        progressTrail={progressTrail}
        s={s}
      />
      <ActionBar
        left={
          <Button
            variant="danger"
            icon={cancelling ? <LoaderCircle className="spin" /> : <X />}
            disabled={cancelling}
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
        }
        right={null}
      />
    </div>
  );
}
