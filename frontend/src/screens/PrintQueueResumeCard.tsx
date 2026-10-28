import { RotateCcw } from "lucide-react";

import { Button } from "../components/ui";
import { type Strings } from "../i18n";
import { type PrintQueueSummary } from "../types";

export function PrintQueueResumeCard({
  queue,
  s,
  onDiscard,
  onResume,
}: {
  queue: PrintQueueSummary;
  s: Strings;
  onDiscard: (queueId: string) => unknown;
  onResume: (queueId: string) => unknown;
}) {
  return (
    <div className="resume-queue-card">
      <div>
        <strong>{s.t("print.resume.title")}</strong>
        <small>
          {s.t("print.resume.summary", {
            count: queue.resumable,
          })}
        </small>
      </div>
      <span className="resume-queue-actions">
        <Button variant="ghost" onClick={() => void onDiscard(queue.queueId)}>
          {s.t("print.resume.discard")}
        </Button>
        <Button
          variant="secondary"
          icon={<RotateCcw />}
          onClick={() => void onResume(queue.queueId)}
        >
          {s.t("print.resume.review")}
        </Button>
      </span>
    </div>
  );
}
