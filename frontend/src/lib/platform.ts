import type { OperationPreset, PathSelection } from "../types";
import type { Bridge } from "./bridge";

async function selectPaths(
  bridge: Bridge,
  opId: string,
  paramName: string,
  droppedPaths?: string[],
): Promise<PathSelection[]> {
  return bridge.call("paths/select", { opId, paramName, droppedPaths });
}

export async function chooseFolder(
  bridge: Bridge,
  opId: string,
  paramName: string,
): Promise<PathSelection | null> {
  return (await selectPaths(bridge, opId, paramName))[0] ?? null;
}

export async function chooseFile(
  bridge: Bridge,
  opId: string,
  paramName: string,
): Promise<PathSelection | null> {
  return (await selectPaths(bridge, opId, paramName))[0] ?? null;
}

export async function chooseFiles(
  bridge: Bridge,
  opId: string,
  paramName: string,
): Promise<PathSelection[]> {
  return selectPaths(bridge, opId, paramName);
}

export async function dropPaths(
  bridge: Bridge,
  opId: string,
  paramName: string,
  paths: string[],
): Promise<PathSelection[]> {
  return selectPaths(bridge, opId, paramName, paths);
}

export async function revealPath(
  bridge: Bridge,
  selectionId: string,
): Promise<void> {
  await bridge.call("paths/reveal", { selectionId });
}

export async function listOperationPresets(
  bridge: Bridge,
  opId: string,
): Promise<OperationPreset[]> {
  return bridge.call("settings/presets", { opId });
}

export async function saveOperationPreset(
  bridge: Bridge,
  params: {
    opId: string;
    name: string;
    values: Record<string, unknown>;
    replace?: boolean;
  },
): Promise<OperationPreset> {
  return bridge.call("settings/savePreset", params);
}

export async function deleteOperationPreset(
  bridge: Bridge,
  opId: string,
  presetId: string,
): Promise<void> {
  await bridge.call("settings/deletePreset", { opId, presetId });
}
