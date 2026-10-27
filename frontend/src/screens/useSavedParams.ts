import { type Dispatch, type SetStateAction, useEffect, useRef, useState } from "react";

import type { Bridge } from "../lib/bridge";
import { persistableValues } from "../lib/operationValues";
import type { OperationDef } from "../types";
import { rememberedParams } from "./operationState";

/**
 * Restores the last remembered values for an operation and autosaves edits.
 * `valuesRevision` is bumped by any action that replaces the values wholesale,
 * so a slow restore cannot overwrite a newer choice.
 */
export function useSavedParams({
  bridge,
  operation,
  initialValues,
  values,
  defaults,
  setValues,
}: {
  bridge: Bridge;
  operation: OperationDef;
  initialValues?: Record<string, unknown>;
  values: Record<string, unknown>;
  defaults: Record<string, unknown>;
  setValues: Dispatch<SetStateAction<Record<string, unknown>>>;
}) {
  const valuesRevision = useRef(0);
  const [hydrated, setHydrated] = useState(false);
  const lastSaved = useRef("");
  const [previousValues, setPreviousValues] = useState<Record<
    string,
    unknown
  > | null>(null);

  useEffect(() => {
    setHydrated(false);
    setPreviousValues(null);
    const hydrationRevision = ++valuesRevision.current;
    void bridge
      .call("settings/params", {
        opId: operation.id,
      })
      .then((saved) => {
        if (valuesRevision.current !== hydrationRevision) {
          setHydrated(true);
          return;
        }
        const filtered = rememberedParams(operation, saved);
        setPreviousValues(
          operation.supportsPresets && Object.keys(filtered).length > 0
            ? filtered
            : null,
        );
        setValues((current) => ({
          ...current,
          ...(operation.supportsPresets ? {} : filtered),
          ...(initialValues ?? {}),
        }));
        lastSaved.current = JSON.stringify(
          persistableValues(operation.schema, {
            ...defaults,
            ...filtered,
            ...(initialValues ?? {}),
          }),
        );
        setHydrated(true);
      })
      .catch(() => {
        setHydrated(true);
      });
    // `defaults` is rebuilt from the operation schema on every render, so
    // the operation identity is the real dependency.
  }, [bridge, operation, initialValues]);

  useEffect(() => {
    // Preset-capable procedures start from clean defaults. Their safe scalar
    // settings are captured only when Preview is requested and are restored
    // later solely through the explicit previous-settings action.
    if (!hydrated || operation.supportsPresets) return;
    const sanitized = persistableValues(operation.schema, values);
    const serialized = JSON.stringify(sanitized);
    if (serialized === lastSaved.current) return;

    const timer = setTimeout(() => {
      lastSaved.current = serialized;
      void bridge
        .call("settings/saveParams", {
          opId: operation.id,
          values: sanitized,
        })
        .catch(() => {});
    }, 200);
    return () => clearTimeout(timer);
  }, [bridge, operation, values, hydrated]);

  return { valuesRevision, lastSaved, previousValues, setPreviousValues };
}
