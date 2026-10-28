import { Button } from "../components/ui";
import { Notice } from "../components/feedback";
import type { Strings } from "../i18n";
import type { RfiPrintCandidate, RfiPrintResolution } from "../types";

/** Results of the batch-print RFI finder: per-request notes and selectable files. */
export function RfiPrintResults({
  resolutions,
  candidates,
  truncated,
  wasRun,
  onSetGroup,
  s,
}: {
  resolutions: RfiPrintResolution[];
  candidates: RfiPrintCandidate[];
  truncated: boolean;
  wasRun: boolean;
  onSetGroup: (predicate: (candidate: RfiPrintCandidate) => boolean, selected: boolean) => void;
  s: Strings;
}) {
  if (resolutions.length === 0) return null;
  return (
    <div className="rfi-print-results">
      <p>{s.t("print.rfi.found", { count: candidates.length })}</p>
      {truncated && <Notice kind="warning">{s.t("print.rfi.truncated")}</Notice>}
      {resolutions.map((resolution, index) => (
        <div className="rfi-print-resolution" key={`${resolution.requested}-${index}`}>
          <strong>{resolution.requested}</strong>
          {resolution.ambiguous && (
            <p role="status">
              {s.t("print.rfi.ambiguous")}
              {resolution.rfiCodes.length > 0 && ` — ${resolution.rfiCodes.join(", ")}`}
            </p>
          )}
          {resolution.missingFirstPage && <p>{s.t("print.rfi.missingFirstPage")}</p>}
          {resolution.missingSketch && <p>{s.t("print.rfi.missingSketch")}</p>}
          <Button
            variant="ghost"
            onClick={() =>
              onSetGroup(
                (candidate) => candidate.requested === resolution.requested,
                !candidates
                  .filter((candidate) => candidate.requested === resolution.requested)
                  .every((candidate) => candidate.selected),
              )
            }
          >
            {s.t("print.rfi.toggleRequest")}
          </Button>
        </div>
      ))}
      {wasRun && (
        <>
          <div className="rfi-print-actions">
            <Button variant="ghost" onClick={() => onSetGroup(() => true, true)}>
              {s.t("action.selectAll")}
            </Button>
            <Button variant="ghost" onClick={() => onSetGroup(() => true, false)}>
              {s.t("action.selectNone")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => onSetGroup((candidate) => candidate.documentType === "firstPage", true)}
            >
              {s.t("print.rfi.selectTypeFirstPage")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => onSetGroup((candidate) => candidate.documentType === "sketch", true)}
            >
              {s.t("print.rfi.selectTypeSketch")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => onSetGroup((candidate) => candidate.documentType === "firstPage", false)}
            >
              {s.t("print.rfi.clearTypeFirstPage")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => onSetGroup((candidate) => candidate.documentType === "sketch", false)}
            >
              {s.t("print.rfi.clearTypeSketch")}
            </Button>
          </div>
          {[...new Set(candidates.map((candidate) => candidate.rfiCode.match(/-RFI-([A-Z0-9]+)-/)?.[1]).filter((discipline): discipline is string => Boolean(discipline)))].map((discipline) => (
            <Button
              key={discipline}
              variant="ghost"
              onClick={() => onSetGroup((candidate) => candidate.rfiCode.includes(`-RFI-${discipline}-`), true)}
            >
              {s.t("print.rfi.selectDiscipline", { discipline })}
            </Button>
          ))}
        </>
      )}
      {candidates.map((candidate) => (
        <label
          className="rfi-print-row"
          key={`${candidate.requested}|${candidate.rfiCode}|${candidate.documentType}|${candidate.displayPath}`}
        >
          <input
            type="checkbox"
            checked={candidate.selected}
            aria-label={`${candidate.rfiCode} ${s.t(`print.rfi.type.${candidate.documentType}`)} ${candidate.displayPath}`}
            onChange={(event) =>
              onSetGroup(
                (row) =>
                  row.requested === candidate.requested &&
                  row.rfiCode === candidate.rfiCode &&
                  row.documentType === candidate.documentType &&
                  row.displayPath === candidate.displayPath,
                event.target.checked,
              )
            }
          />
          <span>
            <strong>{candidate.rfiCode}</strong> — {s.t(`print.rfi.type.${candidate.documentType}`)}
            <small>{candidate.displayPath}</small>
          </span>
        </label>
      ))}
      {candidates.length === 0 && <p>{s.t("print.rfi.noMatches")}</p>}
    </div>
  );
}
