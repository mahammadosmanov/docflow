import { RotateCcw } from "lucide-react";

import { Button } from "../components/ui";
import { type Strings } from "../i18n";

export function PreviousValuesCard({
  s,
  onApply,
}: {
  s: Strings;
  onApply: () => void;
}) {
  return (
    <div className="resume-queue-card">
      <div>
        <strong>{s.t("params.previous.title")}</strong>
        <small>{s.t("params.previous.help")}</small>
      </div>
      <Button variant="secondary" icon={<RotateCcw />} onClick={onApply}>
        {s.t("params.previous.apply")}
      </Button>
    </div>
  );
}
