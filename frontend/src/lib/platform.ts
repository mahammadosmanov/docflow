import type { PathSelection } from "../types";
import type { Bridge } from "./bridge";

async function selectPaths(
  bridge: Bridge,
  opId: string,
  paramName: string,
  fromDrop?: boolean,
): Promise<PathSelection[]> {
  return bridge.call(
    "paths/select",
    fromDrop ? { opId, paramName, fromDrop } : { opId, paramName },
  );
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

/**
 * Claims the files or folder the operator just dropped on the window. Rust
 * captured the paths from the native drop event; the webview sends none.
 */
export async function dropPaths(
  bridge: Bridge,
  opId: string,
  paramName: string,
): Promise<PathSelection[]> {
  return selectPaths(bridge, opId, paramName, true);
}

export type NativeDrop = { type: "over" | "drop" | "leave"; target: Element | null };

/**
 * Reports where an OS file drag is over or dropped in the desktop app. The
 * dropped paths stay in Rust. Outside the desktop shell this does nothing.
 */
export function listenNativeDrop(listener: (drop: NativeDrop) => void): () => void {
  if (!window.__TAURI_INTERNALS__) return () => {};
  let stopped = false;
  let unlisten: (() => void) | undefined;
  void import("@tauri-apps/api/webview")
    .then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent(({ payload }) => {
        if (payload.type === "leave") {
          listener({ type: "leave", target: null });
          return;
        }
        const { x, y } = payload.position.toLogical(window.devicePixelRatio);
        listener({
          type: payload.type === "drop" ? "drop" : "over",
          target: document.elementFromPoint(x, y),
        });
      }),
    )
    .then((stop) => {
      if (stopped) stop();
      else unlisten = stop;
    })
    .catch(() => {});
  return () => {
    stopped = true;
    unlisten?.();
  };
}

export async function revealPath(
  bridge: Bridge,
  selectionId: string,
): Promise<void> {
  await bridge.call("paths/reveal", { selectionId });
}
