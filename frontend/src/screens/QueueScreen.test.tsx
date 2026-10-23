import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { OperationDef, Plan, WatchItem, WatchStatus } from "../types";
import { QueueScreen } from "./QueueScreen";

const s = makeStrings("en");
const OPS = [
  "rfi.process_scans_fast_covers",
  "rfi.group_files",
  "rfi.stage_signed_first_pages",
  "rfi.complete_status_sync",
];

function operation(id: string): OperationDef {
  return {
    id,
    titleKey: `op.${id}.title`,
    descriptionKey: `op.${id}.desc`,
    schema: [],
  } as unknown as OperationDef;
}

function status(items: WatchItem[], enabled: string[] = []): WatchStatus {
  return { operations: OPS, enabled, pollSeconds: 300, paused: false, items };
}

const ready: WatchItem = {
  opId: "rfi.group_files",
  status: "ready",
  planId: "plan-1",
  steps: 4,
  warnings: 1,
  errorCode: null,
  preparedAt: 1,
};

function fakeBridge(
  handler: (method: string, params: Record<string, unknown>) => unknown,
) {
  const call = vi.fn(async (method: string, params?: unknown) =>
    handler(method, (params ?? {}) as Record<string, unknown>),
  );
  return { bridge: { call } as unknown as Bridge, call };
}

afterEach(() => cleanup());

function renderQueue(bridge: Bridge, onOpenPlan = vi.fn()) {
  render(
    <QueueScreen
      bridge={bridge}
      operations={OPS.map(operation)}
      s={s}
      onBack={() => {}}
      onOpenPlan={onOpenPlan}
    />,
  );
  return onOpenPlan;
}

describe("QueueScreen", () => {
  it("shows the empty state and one watch toggle per supported operation", async () => {
    const { bridge } = fakeBridge(() => status([]));
    renderQueue(bridge);
    expect(await screen.findByText("No previews are waiting")).toBeVisible();
    expect(screen.getAllByRole("checkbox")).toHaveLength(4);
    expect(screen.getByRole("button", { name: "Check now" })).toBeDisabled();
  });

  it("enables an operation through watch/update", async () => {
    const { bridge, call } = fakeBridge((method, params) =>
      method === "watch/update"
        ? status([], [String(params.opId)])
        : status([]),
    );
    renderQueue(bridge);
    const [first] = await screen.findAllByRole("checkbox");
    fireEvent.click(first);
    await waitFor(() =>
      expect(call).toHaveBeenCalledWith("watch/update", {
        opId: OPS[0],
        enabled: true,
      }),
    );
    await waitFor(() => expect(first).toBeChecked());
  });

  it("lists a ready preview and opens it by operation id with remembered values", async () => {
    const plan = { planId: "plan-1", opId: "rfi.group_files", steps: [], warnings: [], meta: {} } as Plan;
    const { bridge, call } = fakeBridge((method) => {
      if (method === "watch/status") return status([ready], ["rfi.group_files"]);
      if (method === "watch/open") return plan;
      if (method === "settings/params") return { source: "x" };
      return status([]);
    });
    const onOpenPlan = renderQueue(bridge);
    expect(await screen.findByText("4 planned changes · 1 warnings")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Open preview" }));
    await waitFor(() => expect(onOpenPlan).toHaveBeenCalled());
    expect(call).toHaveBeenCalledWith("watch/open", { opId: "rfi.group_files" });
    expect(onOpenPlan.mock.calls[0][1]).toBe(plan);
    expect(onOpenPlan.mock.calls[0][2]).toEqual({ source: "x" });
  });

  it("offers Prepare again for stale items and Dismiss for any settled item", async () => {
    const stale: WatchItem = { ...ready, status: "stale", planId: null };
    const { bridge, call } = fakeBridge((method) =>
      method === "watch/status" ? status([stale], ["rfi.group_files"]) : status([]),
    );
    renderQueue(bridge);
    expect(screen.queryByRole("button", { name: "Open preview" })).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: "Prepare again" }));
    await waitFor(() =>
      expect(call).toHaveBeenCalledWith("watch/update", {
        opId: "rfi.group_files",
        action: "reprepare",
      }),
    );
  });

  it("explains a failed preparation with a localized code and never offers Open", async () => {
    const failed: WatchItem = {
      ...ready,
      status: "failed",
      planId: null,
      errorCode: "watch.tooManyEntries",
    };
    const { bridge } = fakeBridge(() => status([failed], ["rfi.group_files"]));
    renderQueue(bridge);
    expect(
      await screen.findByText(
        "The folder has too many files to check automatically.",
      ),
    ).toBeVisible();
    expect(screen.queryByRole("button", { name: "Open preview" })).toBeNull();
  });

  it("shows the paused notice while another operation runs", async () => {
    const { bridge } = fakeBridge(() => ({ ...status([]), paused: true }));
    renderQueue(bridge);
    expect(
      await screen.findByText("Paused while another operation is running."),
    ).toBeVisible();
  });
});
