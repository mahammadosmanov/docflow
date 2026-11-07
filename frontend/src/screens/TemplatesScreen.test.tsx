import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { TemplateStatus } from "../types";
import { TemplatesScreen } from "./TemplatesScreen";

const s = makeStrings("en");

const status = (overrides: Partial<TemplateStatus> = {}): TemplateStatus => ({
  online: false,
  syncing: false,
  lastSyncedAt: 1_793_990_000,
  lastError: "offline",
  failedCodes: [],
  templates: [
    {
      code: "QM-FRM-001",
      title: "RFI Form",
      discipline: "GEN",
      fileKind: "xlsx",
      version: 3,
      publishedAt: null,
      fieldCount: 18,
      cached: true,
    },
    {
      code: "CON-ELE-004",
      title: "Earthing System Resistance Test Report",
      discipline: "ELE",
      fileKind: "docx",
      version: 2,
      publishedAt: null,
      fieldCount: 9,
      cached: false,
    },
  ],
  ...overrides,
});

function bridgeWith(result: TemplateStatus) {
  const call = vi.fn(async () => result);
  return { bridge: { call } as unknown as Bridge, call };
}

describe("TemplatesScreen", () => {
  afterEach(cleanup);

  it("loads the cached library, then checks for updates", async () => {
    const { bridge, call } = bridgeWith(status());
    render(<TemplatesScreen bridge={bridge} s={s} onBack={vi.fn()} />);

    expect(await screen.findByText("QM-FRM-001")).toBeInTheDocument();
    expect(call).toHaveBeenNthCalledWith(1, "templates/status");
    await vi.waitFor(() =>
      expect(call).toHaveBeenNthCalledWith(2, "templates/sync"),
    );
    expect(screen.getByRole("status")).toHaveTextContent(/Offline/);
    expect(screen.getByText("Not downloaded")).toBeInTheDocument();
  });

  it("filters by discipline and search text", async () => {
    const { bridge } = bridgeWith(status({ online: true, lastError: null }));
    render(<TemplatesScreen bridge={bridge} s={s} onBack={vi.fn()} />);
    await screen.findByText("QM-FRM-001");

    fireEvent.click(screen.getByRole("button", { name: /ELE/ }));
    expect(screen.queryByText("QM-FRM-001")).not.toBeInTheDocument();
    expect(screen.getByText("CON-ELE-004")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /All/ }));
    fireEvent.change(screen.getByLabelText("Search by code or title…"), {
      target: { value: "rfi" },
    });
    expect(screen.getByText("QM-FRM-001")).toBeInTheDocument();
    expect(screen.queryByText("CON-ELE-004")).not.toBeInTheDocument();
  });

  it("explains when the shared store is not set up", async () => {
    const { bridge } = bridgeWith(
      status({ lastError: "notConfigured", templates: [], lastSyncedAt: null }),
    );
    render(<TemplatesScreen bridge={bridge} s={s} onBack={vi.fn()} />);
    expect(
      await screen.findByText(/An admin must run docs\/supabase_templates.sql/),
    ).toBeInTheDocument();
    expect(screen.getByText("No templates yet")).toBeInTheDocument();
  });
});
