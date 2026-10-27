import { describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { App } from "./App";
import { makeStrings } from "./i18n";
import { formatConfidencePercent } from "./plan/EditablePlanTable";
import { PlanView } from "./plan/PlanView";
import { summarizePlanImpact } from "./plan/PlanSummaries";
import { MockBridge, resetBridgeForTests } from "./lib/bridge";
import type { Plan } from "./types";
import { interceptMockBridge, signIn, registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: formatting and presets", () => {
  it("formats bounded OCR evidence as a score out of 100", () => {
    expect(formatConfidencePercent(1)).toBe("100%");
    expect(formatConfidencePercent(0.804)).toBe("80%");
    expect(formatConfidencePercent(18.42, "percent")).toBe("18%");
    expect(formatConfidencePercent(2)).toBe("100%");
    expect(formatConfidencePercent(-1)).toBe("0%");
    expect(formatConfidencePercent(" ")).toBeNull();
    expect(formatConfidencePercent(Number.NaN)).toBeNull();
  });

  it("summarizes every approved effect category without counting directories", () => {
    const steps: Plan["steps"] = [
      "mkdir",
      "copy",
      "write",
      "pdf_first_page_replace",
      "pdf_page_rotate",
      "pdf_pages_extract",
      "workbook_update",
      "restore_backup",
      "move",
      "rename",
      "print",
      "outlook_reply_draft",
      "cleanup_temp",
    ].map((kind) => ({
      kind: kind as Plan["steps"][number]["kind"],
      src: null,
      dst: null,
      noteKey: null,
      meta: {},
    }));

    expect(
      summarizePlanImpact({
        planId: "impact",
        opId: "test",
        steps,
        warnings: [
          { key: "warn.one", subject: "a" },
          { key: "warn.two", subject: "b" },
        ],
        meta: {},
      }),
    ).toEqual({
      outputs: 5,
      updates: 2,
      moves: 2,
      printJobs: 1,
      drafts: 1,
      cleanup: 1,
      warnings: 2,
    });
  });

  it("shows durable incremental scan reuse without treating it as an apply effect", () => {
    render(
      <PlanView
        plan={{
          planId: "scan-index",
          opId: "rfi.normalize_pdf_orientation",
          steps: [],
          warnings: [],
          meta: {
            scanIndex: {
              discovered: 120,
              reused: 116,
              analyzed: 4,
              failed: 0,
              priorInterruptedRuns: 1,
              forceRescan: false,
            },
          },
        }}
        s={makeStrings("en")}
        error=""
        onCancel={vi.fn()}
        onApply={vi.fn()}
      />,
    );

    expect(screen.getByText("Incremental scan index")).toBeVisible();
    expect(screen.getByText("Reused")).toBeVisible();
    expect(screen.getByText("116")).toBeVisible();
    expect(screen.getByText("Scanned now")).toBeVisible();
    expect(screen.getByText("4")).toBeVisible();
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
  });

  it("does not expose removed work tracking or device readiness surfaces", async () => {
    const call = vi.spyOn(MockBridge.prototype, "call");

    render(<App />);
    await signIn();

    expect(screen.queryByText("RFI İş Takibi")).not.toBeInTheDocument();
    expect(screen.queryByText("Cihaz hazırlığı")).not.toBeInTheDocument();
    expect(screen.queryByText("RFI işini kaydet")).not.toBeInTheDocument();
    expect(screen.queryByText("RFI iş denetimini dışa aktar")).not.toBeInTheDocument();
    expect(call).not.toHaveBeenCalledWith("support/readiness");
    expect(call).not.toHaveBeenCalledWith("workItems/list");
  });
  it("operation presets load only for capability-enabled operations", async () => {
    const call = vi.spyOn(MockBridge.prototype, "call");

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByText("Hızlı RFI Taraması", { selector: "strong" }),
    );
    await screen.findByRole("heading", { name: "Yapılandır" });
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(0);

    cleanup();
    resetBridgeForTests();
    call.mockClear();

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    await waitFor(() =>
      expect(
        call.mock.calls.filter(([method]) => method === "settings/presets"),
      ).toHaveLength(1),
    );
  });

  it("operation presets apply scalar values while preserving files exactly", async () => {
    const call = interceptMockBridge((method) =>
      method === "settings/presets"
        ? {
            handled: true,
            value: [
              {
                id: "preset-1",
                name: "A3 disconnected",
                values: {
                  printer: "Disconnected Printer",
                  paper: "A3",
                  copies: 4,
                  files: [
                    {
                      selectionId: "forged-selection",
                      displayPath: "/forged.pdf",
                      name: "forged.pdf",
                    },
                  ],
                },
              },
            ],
          }
        : { handled: false },
    );

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");

    fireEvent.change(screen.getByLabelText("Kayıtlı ön ayar"), {
      target: { value: "preset-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Ön ayarı uygula" }));
    expect(screen.getByRole("combobox", { name: /^Yazıcı/ })).toHaveValue(
      "Disconnected Printer",
    );
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    await waitFor(() =>
      expect(
        call.mock.calls.some(([method]) => method === "engine/preview"),
      ).toBe(true),
    );
    const previewCall = call.mock.calls.find(
      ([method]) => method === "engine/preview",
    );
    const previewValues = (
      previewCall?.[1] as { values: Record<string, unknown> }
    ).values;
    expect(previewValues).toMatchObject({
      printer: "Disconnected Printer",
      paper: "A3",
      copies: 4,
    });
    expect(previewValues.files).toStrictEqual([
      {
        selectionId: "demo-selection-files-0",
        displayPath: "/Users/demo/Documents/first.pdf",
        name: "first.pdf",
      },
      {
        selectionId: "demo-selection-files-1",
        displayPath: "/Users/demo/Documents/second.docx",
        name: "second.docx",
      },
    ]);
  });

  it("stale saved params do not replace preset or user form values", async () => {
    let resolveParams!: (values: Record<string, unknown>) => void;
    const paramsResult = new Promise<Record<string, unknown>>((resolve) => {
      resolveParams = resolve;
    });
    interceptMockBridge((method) => {
      if (method === "settings/params") {
        return { handled: true, value: paramsResult };
      }
      if (method === "settings/presets") {
        return {
          handled: true,
          value: [
            {
              id: "preset-1",
              name: "A3 run",
              values: { paper: "A3", copies: 4 },
            },
          ],
        };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.change(await screen.findByLabelText("Kayıtlı ön ayar"), {
      target: { value: "preset-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Ön ayarı uygula" }));
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A3");
    fireEvent.change(
      screen.getByRole("spinbutton", { name: /^Kopya sayısı/ }),
      { target: { value: "7" } },
    );

    await act(async () => {
      resolveParams({ paper: "A4", copies: 2 });
      await paramsResult;
    });

    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A3");
    expect(
      screen.getByRole("spinbutton", { name: /^Kopya sayısı/ }),
    ).toHaveValue(7);
  });

  it("operation presets save only current bool choice and int values", async () => {
    let presets: unknown[] = [];
    const savedParams: { value?: Record<string, unknown> } = {};
    const call = interceptMockBridge((method, params) => {
      if (method === "settings/presets") {
        return { handled: true, value: presets };
      }
      if (method === "settings/savePreset") {
        savedParams.value = params;
        const saved = {
          id: "preset-saved",
          name: String(params.name),
          values: params.values,
        };
        presets = [saved];
        return { handled: true, value: saved };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.change(
      screen.getByRole("spinbutton", { name: /^Kopya sayısı/ }),
      {
        target: { value: "3" },
      },
    );
    fireEvent.change(screen.getByLabelText("Yeni ön ayar adı"), {
      target: { value: "Site A3" },
    });
    const saveButton = screen.getByRole("button", {
      name: "Ön ayarı kaydet",
    });
    expect(saveButton).toBeEnabled();
    fireEvent.click(saveButton);

    await waitFor(() =>
      expect(call.mock.calls.map(([method]) => method)).toContain(
        "settings/savePreset",
      ),
    );
    expect(savedParams.value).toMatchObject({
      opId: "files.batch_print",
      name: "Site A3",
      values: {
        printer: "Office Printer",
        copies: 3,
        paper: "A4",
        orientation: "auto",
        scaling: "fit",
        color: "printer",
        duplex: "printer",
        collate: true,
        retries: 0,
        on_error: "pause",
        timeout_minutes: 30,
      },
    });
    expect(savedParams.value?.values).not.toHaveProperty("files");
    expect(savedParams.value?.values).not.toHaveProperty("rfi_numbers");
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(2);
  });

  it("operation presets replace a duplicate only after explicit confirmation", async () => {
    const preset = {
      id: "preset-existing",
      name: "A4 standard",
      values: { paper: "A4", copies: 1 },
    };
    const saveRequests: Record<string, unknown>[] = [];
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const call = interceptMockBridge((method, params) => {
      if (method === "settings/presets") {
        return { handled: true, value: [preset] };
      }
      if (method === "settings/savePreset") {
        saveRequests.push(params);
        return params.replace === true
          ? { handled: true, value: preset }
          : {
              handled: true,
              value: Promise.reject(new Error("error.preset.duplicate")),
            };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.change(await screen.findByLabelText("Yeni ön ayar adı"), {
      target: { value: "A4 standard" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Ön ayarı kaydet" }));

    await waitFor(() => expect(saveRequests).toHaveLength(2));
    expect(confirm).toHaveBeenCalledWith(
      "A4 standard adlı ön ayar zaten var. Değiştirilsin mi?",
    );
    expect(saveRequests[0]).not.toHaveProperty("replace");
    expect(saveRequests[1]).toMatchObject({
      opId: "files.batch_print",
      name: "A4 standard",
      replace: true,
    });
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(2);
  });

  it("operation presets delete only after confirmation and command success", async () => {
    const first = {
      id: "preset-first",
      name: "A3 duplex",
      values: { paper: "A3", duplex: "long" },
    };
    const second = {
      id: "preset-second",
      name: "A4 simplex",
      values: { paper: "A4", duplex: "simplex" },
    };
    let presets = [first, second];
    let resolveDelete!: () => void;
    const deleteResult = new Promise<{ ok: true }>((resolve) => {
      resolveDelete = () => {
        presets = [second];
        resolve({ ok: true });
      };
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const call = interceptMockBridge((method, params) => {
      if (method === "settings/presets") {
        return { handled: true, value: presets };
      }
      if (method === "settings/deletePreset") {
        expect(confirm).toHaveBeenCalled();
        expect(params).toStrictEqual({
          opId: "files.batch_print",
          presetId: "preset-first",
        });
        return { handled: true, value: deleteResult };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    const select = await screen.findByLabelText("Kayıtlı ön ayar");
    fireEvent.change(select, { target: { value: "preset-first" } });
    fireEvent.click(screen.getByRole("button", { name: "Ön ayarı sil" }));

    expect(confirm).toHaveBeenCalledWith("A3 duplex adlı ön ayar silinsin mi?");
    expect(screen.getByRole("option", { name: "A3 duplex" })).toBeVisible();

    resolveDelete();
    await waitFor(() =>
      expect(screen.queryByRole("option", { name: "A3 duplex" })).toBeNull(),
    );
    expect(screen.getByRole("option", { name: "A4 simplex" })).toBeVisible();
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(2);
  });

  it("operation presets show localized command errors", async () => {
    const writeError = Object.assign(new Error("settings could not be saved"), {
      code: "settings_write_failed",
    });
    const call = interceptMockBridge((method) => {
      if (method === "settings/presets") {
        return { handled: true, value: [] };
      }
      if (method === "settings/savePreset") {
        return {
          handled: true,
          value: Promise.reject(writeError),
        };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.change(await screen.findByLabelText("Yeni ön ayar adı"), {
      target: { value: "Unavailable" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Ön ayarı kaydet" }));

    expect(
      await screen.findByText(
        "Ön ayar ayarları kaydedilemedi. Tekrar deneyin.",
      ),
    ).toBeVisible();
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(1);
  });
});
