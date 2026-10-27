import { useEffect } from "react";

import type { RunStage } from "./useApplyRun";

/** Ctrl/Cmd+Enter runs the next step; Escape steps back. */
export function useOperationShortcuts({
  stage,
  cancelling,
  preview,
  apply,
  cancelPreview,
  setStage,
  onBack,
}: {
  stage: RunStage;
  cancelling: boolean;
  preview: () => void | Promise<void>;
  apply: () => void | Promise<void>;
  cancelPreview: () => void | Promise<void>;
  setStage: (stage: RunStage) => void;
  onBack: () => void;
}) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        if (stage === "form") {
          event.preventDefault();
          void preview();
        } else if (stage === "preview" && !cancelling) {
          event.preventDefault();
          void apply();
        }
      } else if (event.key === "Escape") {
        if (stage === "previewing") {
          event.preventDefault();
          void cancelPreview();
        } else if (stage === "preview" || stage === "result") {
          event.preventDefault();
          setStage("form");
        } else if (stage === "form") {
          event.preventDefault();
          onBack();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [stage, cancelling, preview, apply, cancelPreview, setStage, onBack]);
}
