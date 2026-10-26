import { useCallback, useState } from "react";

import { type ProgressEvent } from "../types";
import { nextProgressTrail } from "./operationState";

/** Live progress for one operation run: the latest event and a short trail of phases. */
export function useProgressTrail() {
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [progressTrail, setProgressTrail] = useState<ProgressEvent[]>([]);
  const receiveProgress = useCallback((event: ProgressEvent) => {
    setProgress(event);
    setProgressTrail((current) => nextProgressTrail(current, event));
  }, []);
  const startProgress = useCallback((event: ProgressEvent) => {
    setProgress(event);
    setProgressTrail([event]);
  }, []);
  return { progress, progressTrail, receiveProgress, startProgress };
}
