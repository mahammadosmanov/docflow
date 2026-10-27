import { useCallback, useEffect, useState } from "react";

import { type Bridge } from "../lib/bridge";
import { type ReplicaAutomationStatus } from "../types";

/** Read-only replica automation status, refreshed on mount and every five seconds. */
export function useReplicaStatus(bridge: Bridge) {
  const [replicaStatus, setReplicaStatus] =
    useState<ReplicaAutomationStatus | null>(null);
  const refreshReplicaStatus = useCallback(async () => {
    try {
      setReplicaStatus(await bridge.call("settings/replicaStatus"));
    } catch {
      // The preference remains usable when a read-only refresh is unavailable.
    }
  }, [bridge]);
  useEffect(() => {
    void refreshReplicaStatus();
    const timer = window.setInterval(() => void refreshReplicaStatus(), 5_000);
    return () => window.clearInterval(timer);
  }, [refreshReplicaStatus]);

  return { replicaStatus, refreshReplicaStatus };
}
