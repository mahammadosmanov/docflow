import { describe, expect, it, vi } from "vitest";

import type { PathSelection } from "../types";
import type { Bridge } from "./bridge";
import {
  chooseFile,
  chooseFiles,
  chooseFolder,
  dropPaths,
  revealPath,
} from "./platform";

const selection = (id: string): PathSelection => ({
  selectionId: id,
  displayPath: `/docs/${id}`,
  name: id,
});

const bridgeReturning = (result: unknown) => {
  const call = vi.fn(async () => result);
  return { bridge: { call } as unknown as Bridge, call };
};

describe("path selection helpers", () => {
  it("chooseFolder and chooseFile return only the first selection", async () => {
    const { bridge, call } = bridgeReturning([selection("a"), selection("b")]);
    await expect(chooseFolder(bridge, "op", "source")).resolves.toEqual(selection("a"));
    await expect(chooseFile(bridge, "op", "workbook")).resolves.toEqual(selection("a"));
    expect(call).toHaveBeenNthCalledWith(1, "paths/select", {
      opId: "op",
      paramName: "source",
      droppedPaths: undefined,
    });
  });

  it("chooseFolder and chooseFile return null when the user picks nothing", async () => {
    const { bridge } = bridgeReturning([]);
    await expect(chooseFolder(bridge, "op", "source")).resolves.toBeNull();
    await expect(chooseFile(bridge, "op", "workbook")).resolves.toBeNull();
  });

  it("chooseFiles returns every selection", async () => {
    const all = [selection("a"), selection("b")];
    const { bridge } = bridgeReturning(all);
    await expect(chooseFiles(bridge, "op", "files")).resolves.toEqual(all);
  });

  it("dropPaths forwards the dropped paths to the bridge", async () => {
    const { bridge, call } = bridgeReturning([selection("a")]);
    await dropPaths(bridge, "op", "files", ["/x.pdf"]);
    expect(call).toHaveBeenCalledWith("paths/select", {
      opId: "op",
      paramName: "files",
      droppedPaths: ["/x.pdf"],
    });
  });

  it("revealPath sends the selection id to the bridge", async () => {
    const { bridge, call } = bridgeReturning(undefined);
    await revealPath(bridge, "sel-1");
    expect(call).toHaveBeenCalledWith("paths/reveal", { selectionId: "sel-1" });
  });
});
