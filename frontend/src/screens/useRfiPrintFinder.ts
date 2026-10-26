import { useRef, useState } from "react";

import type { Bridge } from "../lib/bridge";
import type { Strings } from "../i18n";
import { humanError, isPathSelection } from "../helpers";
import type { RfiPrintCandidate, RfiPrintResolution } from "../types";

/** The batch-print RFI finder: its search, its results, and which typed numbers are included. */
export function useRfiPrintFinder({
  bridge,
  s,
  values,
  rfiInputs,
}: {
  bridge: Bridge;
  s: Strings;
  values: Record<string, unknown>;
  rfiInputs: string[];
}) {
  const [rfiPrintCandidates, setRfiPrintCandidates] = useState<RfiPrintCandidate[]>([]);
  const [rfiPrintResolutions, setRfiPrintResolutions] = useState<RfiPrintResolution[]>([]);
  const [rfiFinderBusy, setRfiFinderBusy] = useState(false);
  const [rfiFinderError, setRfiFinderError] = useState("");
  const [excludedRfiInputs, setExcludedRfiInputs] = useState<string[]>([]);
  const [rfiFinderWasRun, setRfiFinderWasRun] = useState(false);
  const rfiSearchRevision = useRef(0);
  const [rfiSearchTruncated, setRfiSearchTruncated] = useState(false);

  const invalidateRfiFinder = () => {
    rfiSearchRevision.current += 1;
    setRfiFinderBusy(false);
    setRfiFinderError("");
    setRfiFinderWasRun(false);
    setRfiSearchTruncated(false);
    setRfiPrintCandidates([]);
    setRfiPrintResolutions([]);
  };

  const searchRfiDocuments = async () => {
    setRfiFinderError("");
    setRfiFinderWasRun(false);
    setRfiPrintCandidates([]);
    setRfiPrintResolutions([]);
    setRfiSearchTruncated(false);
    const root = values.rfi_system_root;
    const requestRevision = ++rfiSearchRevision.current;
    if (!isPathSelection(root)) {
      setRfiFinderError(s.t("print.rfi.rootRequired"));
      return;
    }
    const numbers = rfiInputs.filter((input) => !excludedRfiInputs.includes(input));
    if (numbers.length === 0) {
      setRfiFinderError(s.t("print.rfi.inputRequired"));
      return;
    }
    if (values.include_first_page !== true && values.include_sketch !== true) {
      setRfiFinderError(s.t("print.rfi.typesRequired"));
      return;
    }
    setRfiFinderBusy(true);
    try {
      const result = await bridge.call("rfi/printFind", {
        root,
        numbers,
        firstPage: values.include_first_page === true,
        sketch: values.include_sketch === true,
      });
      if (requestRevision !== rfiSearchRevision.current) return;
      setRfiPrintCandidates(result.candidates);
      setRfiPrintResolutions(result.resolutions);
      setRfiFinderWasRun(true);
      setRfiSearchTruncated(result.truncated);
    } catch (reason) {
      if (requestRevision === rfiSearchRevision.current) {
        setRfiFinderError(humanError(reason, s));
      }
    } finally {
      if (requestRevision === rfiSearchRevision.current) setRfiFinderBusy(false);
    }
  };

  const setRfiCandidateGroup = (
    predicate: (candidate: RfiPrintCandidate) => boolean,
    selected: boolean,
  ) => {
    setRfiPrintCandidates((current) =>
      current.map((candidate) =>
        predicate(candidate) ? { ...candidate, selected } : candidate,
      ),
    );
  };

  return {
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
  };
}
