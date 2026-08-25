import type { PathSelection } from "../types";
import type { Bridge } from "./bridge";

async function selectPaths(
  bridge: Bridge,
  opId: string,
  paramName: string,
): Promise<PathSelection[]> {
  return bridge.call("paths/select", { opId, paramName });
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

export async function revealPath(
  bridge: Bridge,
  selectionId: string,
): Promise<void> {
  await bridge.call("paths/reveal", { selectionId });
}
