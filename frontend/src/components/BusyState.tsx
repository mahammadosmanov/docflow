import { useEffect, useState } from "react";
import {
  Check,
  LoaderCircle,
} from "lucide-react";

import type { Strings } from "../i18n";
import type { ProgressEvent } from "../types";

export function BusyState({
  label,
  progress,
  progressTrail = [],
  s,
}: {
  label: string;
  progress?: ProgressEvent | null;
  progressTrail?: ProgressEvent[];
  s: Strings;
}) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(
      () => setElapsed(Math.floor((Date.now() - started) / 1000)),
      1000,
    );
    return () => window.clearInterval(timer);
  }, []);
  const determinate = Boolean(progress && progress.total > 0);
  const fraction =
    progress && progress.total > 0 ? progress.done / progress.total : 0;
  return (
    <div className="busy-state">
      <LoaderCircle className="spin" />
      <h2>{label}</h2>
      {progress && (
        <strong className="progress-phase" role="status">
          {s.t(`progress.phase.${progress.phase}`)}
        </strong>
      )}
      <div
        className={determinate ? "progress" : "progress indeterminate"}
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={progress?.total || undefined}
        aria-valuenow={progress && progress.total > 0 ? progress.done : undefined}
      >
        <span style={determinate ? { width: `${fraction * 100}%` } : undefined} />
      </div>
      {progress && (
        <small>
          {progress.total > 0 ? `${progress.done} / ${progress.total}` : ""}
          {progress.subject ? ` · ${progress.subject}` : ""}
        </small>
      )}
      {elapsed >= 3 && (
        <small className="busy-elapsed" role="timer" aria-live="off">
          {s.t("progress.elapsed", { seconds: elapsed })}
        </small>
      )}
      {progressTrail.length > 1 && (
        <ol className="progress-trail" aria-label={s.t("progress.stages")}>
          {progressTrail.map((event, index) => (
            <li
              key={`${event.phase}-${index}`}
              className={index === progressTrail.length - 1 ? "current" : "done"}
            >
              {index === progressTrail.length - 1 ? (
                <LoaderCircle className="spin" aria-hidden="true" />
              ) : (
                <Check aria-hidden="true" />
              )}
              <span>{s.t(`progress.phase.${event.phase}`)}</span>
              {event.total > 0 && (
                <small>
                  {event.done}/{event.total}
                </small>
              )}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
