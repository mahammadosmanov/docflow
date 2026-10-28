import { Button, Notice } from "../components/ui";
import { type Strings } from "../i18n";
import { RfiInputChooser } from "./RfiInputChooser";
import { RfiPrintResults } from "./RfiPrintResults";
import { type useRfiPrintFinder } from "./useRfiPrintFinder";

export function RfiPrintFinderPanel({
  s,
  inputs,
  finder,
}: {
  s: Strings;
  inputs: string[];
  finder: ReturnType<typeof useRfiPrintFinder>;
}) {
  const {
    rfiPrintCandidates,
    rfiPrintResolutions,
    rfiFinderBusy,
    rfiFinderError,
    excludedRfiInputs,
    setExcludedRfiInputs,
    rfiFinderWasRun,
    rfiSearchTruncated,
    invalidateRfiFinder,
    searchRfiDocuments,
    setRfiCandidateGroup,
  } = finder;
  return (
    <section className="rfi-print-finder" aria-labelledby="rfi-print-title">
      <h3 id="rfi-print-title">{s.t("print.rfi.title")}</h3>
      <RfiInputChooser
        inputs={inputs}
        excluded={excludedRfiInputs}
        s={s}
        onSelectAll={() => {
          invalidateRfiFinder();
          setExcludedRfiInputs([]);
        }}
        onSelectNone={() => {
          invalidateRfiFinder();
          setExcludedRfiInputs(inputs);
        }}
        onToggle={(input, checked) => {
          invalidateRfiFinder();
          setExcludedRfiInputs((current) =>
            checked
              ? current.filter((value) => value !== input)
              : [...current, input],
          );
        }}
      />
      <Button
        variant="secondary"
        disabled={rfiFinderBusy || !inputs.length}
        onClick={() => void searchRfiDocuments()}
      >
        {s.t(rfiFinderBusy ? "print.rfi.searching" : "print.rfi.search")}
      </Button>
      {rfiFinderError && <Notice kind="error">{rfiFinderError}</Notice>}
      <RfiPrintResults
        resolutions={rfiPrintResolutions}
        candidates={rfiPrintCandidates}
        truncated={rfiSearchTruncated}
        wasRun={rfiFinderWasRun}
        onSetGroup={setRfiCandidateGroup}
        s={s}
      />
    </section>
  );
}
