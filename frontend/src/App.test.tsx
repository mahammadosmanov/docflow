import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useState } from "react";
import {
  App,
  canonicalRfiInText,
  formatConfidencePercent,
  humanError,
  isPlanEditorReady,
  PlanWarnings,
  PlanView,
  safePngThumbnail,
  summarizePlanImpact,
  summarizeWorkbookCellChanges,
} from "./App";
import { makeStrings } from "./i18n";
import { MockBridge, resetBridgeForTests } from "./lib/bridge";
import type { Plan } from "./types";
import {
  AdjustableDataRegion,
  ColumnResizeHandle,
} from "./components/AdjustableDataRegion";

function interceptMockBridge(
  intercept: (
    method: string,
    params: Record<string, unknown>,
  ) => { handled: boolean; value?: unknown },
) {
  const originalCall = MockBridge.prototype.call;
  return vi.spyOn(MockBridge.prototype, "call").mockImplementation(function (
    this: MockBridge,
    method,
    ...args
  ) {
    const result = intercept(
      method,
      (args[0] ?? {}) as Record<string, unknown>,
    );
    if (result.handled) return Promise.resolve(result.value) as never;
    return Reflect.apply(originalCall, this, [method, ...args]);
  });
}

async function signIn() {
  expect(await screen.findByRole("heading", { name: "DocFlow" })).toBeVisible();
  fireEvent.change(screen.getByLabelText("E-posta"), {
    target: { value: "demo@example.com" },
  });
  fireEvent.change(screen.getByLabelText("Parola"), {
    target: { value: "secret1" },
  });
  const submit = document.querySelector<HTMLButtonElement>(
    'button[type="submit"]',
  );
  expect(submit).not.toBeNull();
  fireEvent.click(submit!);
  await screen.findByRole("heading", { name: "Çalışma alanı" });
}

afterEach(() => {
  cleanup();
  resetBridgeForTests();
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-font");
  document.documentElement.removeAttribute("data-compact");
});

describe("adjustable plan data regions", () => {
  it("resizes columns with keyboard and pointer input within accessible bounds", () => {
    const pointerCaptureDescriptor = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "setPointerCapture",
    );
    Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
      configurable: true,
      value: vi.fn(),
    });
    const setWidth = vi.fn();
    render(
      <ColumnResizeHandle
        width={120}
        label="RFI number"
        s={makeStrings("en")}
        onWidthChange={setWidth}
      />,
    );
    const separator = screen.getByRole("separator", {
      name: "Resize RFI number column",
    });
    fireEvent.keyDown(separator, { key: "ArrowRight" });
    expect(setWidth).toHaveBeenLastCalledWith(136);
    fireEvent.keyDown(separator, { key: "Home" });
    expect(setWidth).toHaveBeenLastCalledWith(48);
    fireEvent.pointerDown(separator, {
      button: 0,
      pointerId: 1,
      clientX: 100,
    });
    fireEvent.pointerMove(separator, { pointerId: 1, clientX: 200 });
    expect(separator).toHaveAttribute("aria-valuenow", "220");
    fireEvent.pointerUp(separator, { pointerId: 1, clientX: 200 });
    expect(setWidth).toHaveBeenLastCalledWith(220);
    fireEvent.pointerDown(separator, {
      button: 0,
      pointerId: 2,
      clientX: 100,
    });
    fireEvent.pointerMove(separator, { pointerId: 2, clientX: 180 });
    fireEvent.pointerCancel(separator, { pointerId: 2 });
    expect(separator).toHaveAttribute("aria-valuenow", "120");
    expect(setWidth).toHaveBeenLastCalledWith(220);
    if (pointerCaptureDescriptor) {
      Object.defineProperty(
        HTMLElement.prototype,
        "setPointerCapture",
        pointerCaptureDescriptor,
      );
    } else {
      delete (HTMLElement.prototype as Partial<HTMLElement>).setPointerCapture;
    }
  });

  it("keeps a usable height minimum and clamps height when the window shrinks", () => {
    const originalHeight = window.innerHeight;
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 700,
    });
    render(
      <AdjustableDataRegion
        s={makeStrings("en")}
        className="test-data"
        label="Results"
      >
        <div>Rows</div>
      </AdjustableDataRegion>,
    );
    let separator = screen.getByRole("separator", {
      name: "Resize Results height",
    });
    fireEvent.keyDown(separator, { key: "Home" });
    fireEvent.keyDown(separator, { key: "ArrowDown" });
    expect(separator).toHaveAttribute("aria-valuenow", "156");
    fireEvent.keyDown(separator, { key: "ArrowUp" });
    expect(separator).toHaveAttribute("aria-valuenow", "140");
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 300,
    });
    fireEvent(window, new Event("resize"));
    separator = screen.getByRole("separator", {
      name: "Resize Results height",
    });
    expect(separator).toHaveAttribute("aria-valuemin", "140");
    expect(separator).toHaveAttribute("aria-valuenow", "140");
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: originalHeight,
    });
    fireEvent(window, new Event("resize"));
  });
  it("retains edited and selected rows and column width across filtering, detail toggles, and revision", () => {
    const initialRows = [
      { _rowId: "row-1", rfiNo: "MEC-0700", discipline: "MEC" },
    ];
    const basePlan: Plan = {
      planId: "adjustment-plan-0",
      opId: "rfi.system_sync",
      steps: [],
      warnings: [],
      meta: {
        editor: {
          kind: "recordTable",
          titleKey: "rfi.systemSync.editor.title",
          requiredFields: ["rfiNo"],
          selectionEnabled: true,
          detailsEnabled: true,
          detailsDefaultVisible: true,
          columns: [
            {
              key: "rfiNo",
              labelKey: "rfi.systemSync.column.rfiNo",
              kind: "text",
            },
            {
              key: "discipline",
              labelKey: "rfi.systemSync.column.discipline",
              kind: "text",
              detail: true,
            },
          ],
        },
      },
    };
    const StatefulPlanView = ({ revision }: { revision: number }) => {
      const [rows, setRows] = useState<Record<string, unknown>[]>(initialRows);
      const editor = {
        ...(basePlan.meta?.editor as Record<string, unknown>),
        rows,
      };
      const plan = {
        ...basePlan,
        planId: `adjustment-plan-${revision}`,
        meta: { editor },
      };
      return (
        <PlanView
          plan={plan}
          s={makeStrings("en")}
          error=""
          onCancel={vi.fn()}
          onApply={vi.fn()}
          editorRows={rows}
          onEditorRowsChange={setRows}
          editorDirty
          onUpdatePreview={vi.fn()}
        />
      );
    };

    const view = render(<StatefulPlanView revision={0} />);
    const rfiNo = screen.getByRole("textbox", { name: "RFI No." });
    fireEvent.change(rfiNo, { target: { value: "MEC-0701" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Select row 1" }));
    let resize = screen.getByRole("separator", {
      name: "Resize RFI No. column",
    });
    fireEvent.keyDown(resize, { key: "ArrowRight" });
    expect(resize).toHaveAttribute("aria-valuenow", "136");
    fireEvent.click(screen.getByRole("button", { name: "Hide automatic fields" }));
    fireEvent.click(screen.getByRole("button", { name: "Show automatic fields" }));
    fireEvent.change(screen.getByRole("searchbox"), {
      target: { value: "not a matching RFI" },
    });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
    view.rerender(<StatefulPlanView revision={1} />);

    expect(screen.getByRole("textbox", { name: "RFI No." })).toHaveValue("MEC-0701");
    expect(screen.getByRole("checkbox", { name: "Select row 1" })).toBeChecked();
    resize = screen.getByRole("separator", {
      name: "Resize RFI No. column",
    });
    expect(resize).toHaveAttribute("aria-valuenow", "136");
  });
});

describe("DocFlow shell", () => {
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

  it("localizes the invalid RFI workbook error in every supported language", () => {
    const expected = {
      tr: "Geçerli bir makro etkin .xlsm veya standart .xlsx RFI Log dosyası seç.",
      en: "Select a valid macro-enabled .xlsm or standard .xlsx RFI Log workbook.",
      az: "Etibarlı makro aktiv .xlsm və ya standart .xlsx RFI Log faylı seçin.",
      ru: "Выберите допустимый RFI Log с поддержкой макросов .xlsm или стандартный .xlsx.",
    } as const;
    const followUpExpected = {
      tr: "Geçerli RFI Log ve Follow Up .xlsm veya .xlsx çalışma kitapları seç.",
      en: "Select valid RFI Log and Follow Up .xlsm or .xlsx workbooks.",
      az: "Etibarlı RFI Log və Follow Up .xlsm və ya .xlsx iş kitabları seçin.",
      ru: "Выберите допустимые книги RFI Log и Follow Up в формате .xlsm или .xlsx.",
    } as const;

    for (const [locale, message] of Object.entries(expected)) {
      const strings = makeStrings(locale as keyof typeof expected);
      expect(humanError(new Error("error.rfi.workbookInvalid"), strings)).toBe(
        message,
      );
      expect(
        humanError(new Error("error.rfi.followUp.workbookInvalid"), strings),
      ).toBe(followUpExpected[locale as keyof typeof followUpExpected]);
      expect(
        humanError(
          new Error("error.rfi.followUp.workbookInvalid: file check failed"),
          strings,
        ),
      ).toBe(
        `${followUpExpected[locale as keyof typeof followUpExpected]} (file check failed)`,
      );
    }
  });

  it("localizes the legacy print queue reselection migration in every supported language", () => {
    const expected = {
      tr: "Bu eski yazdırma kuyruğu güvenle devam ettirilemez. Kuyruğu silip dosyaları yeniden seç.",
      en: "This older print queue cannot be resumed safely. Discard it, then reselect the files.",
      az: "Bu köhnə çap növbəsini təhlükəsiz davam etdirmək olmur. Növbəni silin, sonra faylları yenidən seçin.",
      ru: "Эту старую очередь печати нельзя безопасно продолжить. Удалите её и заново выберите файлы.",
    } as const;

    for (const [locale, message] of Object.entries(expected)) {
      expect(
        humanError(
          new Error("error.print.queueSourceReselectionRequired"),
          makeStrings(locale as keyof typeof expected),
        ),
      ).toBe(message);
    }
  });

  it("localizes every reachable Excel source export and cleanup error key", () => {
    const keys = [
      "error.print.excelExportTimedOut",
      "error.print.excelExportCancelled",
      "error.print.excelExportQuarantined",
      "error.print.exportCleanupRequiresReview",
      "error.print.excelSourceMotwBlocked",
      "error.print.excelWorkbookUnsafe",
      "error.print.excelComContractInvalid",
      "error.print.excelComFailed",
      "error.print.excelExportDestinationExists",
      "error.print.excelExportFailed",
      "error.print.excelExportMalformedPdf",
      "error.print.excelPrinterContextUnavailable",
      "error.print.excelPrintJobRejected",
      "error.print.excelPrintCompletionUnconfirmed",
      "error.print.excelControlledPdfUnavailable",
      "error.print.excelWorkerStartFailed",
      "error.print.excelDeadlineInvalid",
      "error.print.excelWorksheetMetadataMissing",
      "error.print.excelSafetyMetadataMissing",
      "error.print.excelGenerationExhausted",
      "error.print.excelWorkerStopped",
      "error.print.excelResponseStale",
      "error.print.excelStagingUnavailable",
      "error.print.excelSourceNameUnavailable",
      "error.print.excelStagingFailed",
      "error.print.excelStagingVerificationFailed",
      "error.print.excelPrivatePdfUnavailable",
      "error.print.excelPrivatePdfEmpty",
    ];

    for (const locale of ["tr", "en", "ru", "az"] as const) {
      const strings = makeStrings(locale);
      for (const key of keys) {
        const localized = strings.t(key);
        expect(localized, `${locale}:${key}`).not.toBe(key);
        expect(humanError(new Error(key), strings)).toBe(localized);
      }
    }
  });

  it("requires an archive choice for every enabled ambiguous signed-page row", () => {
    const editor = { requiredFields: ["archiveFile"] };
    expect(
      isPlanEditorReady(editor, [
        { enabled: true, archiveFile: "" },
        { enabled: true, archiveFile: "Package/RFI No. CODE.pdf" },
      ]),
    ).toBe(false);
    expect(
      isPlanEditorReady(editor, [
        { enabled: true, archiveFile: "Other/RFI No. CODE.pdf" },
      ]),
    ).toBe(true);
    expect(
      isPlanEditorReady(editor, [{ enabled: false, archiveFile: "" }]),
    ).toBe(true);
  });
  it("normalizes short Fast RFI Scan edits when rebuilding destinations", async () => {
    const bridge = new MockBridge();
    const plan = await bridge.call("engine/preview", {
      opId: "rfi.process_scans_fast_covers",
      values: {
        source: { displayPath: "/isolated/scans" },
        dest: { displayPath: "/isolated/output" },
        organize_into_rfi_folders: true,
        scan_mode: "multi_rfi",
      },
    });
    const editor = plan.meta.editor as Record<string, unknown>;
    const rows = editor.rows as Record<string, unknown>[];
    const sourcePath = plan.steps[0].src;
    const revised = await bridge.call("engine/revise", {
      planId: plan.planId,
      rows: rows.map((row) => ({
        ...row,
        code: "AZE-SUN-INT-QMS-RFI-MEC-901",
      })),
    });

    expect(revised.steps[0].src).toBe(sourcePath);
    expect(revised.steps[0].dst).toBe(
      "/isolated/output/MEC/RFI No. AZE-SUN-INT-QMS-RFI-MEC-0901.pdf",
    );
    expect(
      (revised.meta.editor as Record<string, unknown>).rows,
    ).toMatchObject([
      {
        code: "AZE-SUN-INT-QMS-RFI-MEC-0901",
        target:
          "/isolated/output/MEC/RFI No. AZE-SUN-INT-QMS-RFI-MEC-0901.pdf",
      },
    ]);
  });

  it("accepts only exact supported RFI candidates with one-to-four-digit suffixes", () => {
    const editor = { kind: "rfiScan", requiredFields: ["code"] };
    for (const suffix of ["9", "91", "901", "0901"]) {
      expect(
        isPlanEditorReady(editor, [
          { code: `AZE-SUN-INT-QMS-RFI-CIV-${suffix}` },
        ]),
      ).toBe(true);
    }
    for (const code of [
      "AZE-SUN-INT-QMS-RFI-CLV-0192",
      "aze-sun-int-qms-rfi-civ-0192",
      "AZE-SUN-INT-QMS-RFI-CIV-09010",
      "AZE-SUN-INT-QMS-ITP-CIV-901",
      "2026-09-01",
      "AZE-SUN-INT-QMS-RFI-CIV-901A",
      "AZE-SUN-INT-QMS-RFI-CIV-901А",
    ]) {
      expect(isPlanEditorReady(editor, [{ code }])).toBe(false);
    }
    expect(
      isPlanEditorReady(editor, [
        { code: "AZE-SUN-INT-QMS-RFI-CIV-0192" },
        { code: "AZE-SUN-INT-QMS-RFI-CIV-0192" },
      ]),
    ).toBe(false);
  });

  it("updates the Fast RFI Scan preview before allowing Apply", () => {
    const plan: Plan = {
      planId: "fast-scan-plan",
      opId: "rfi.process_scans_fast_covers",
      steps: [
        {
          kind: "copy",
          src: "/incoming/scan.pdf",
          dst: "/destination/CIV/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0192.pdf",
          noteKey: "note.renamed",
          meta: { code: "AZE-SUN-INT-QMS-RFI-CIV-0192" },
        },
      ],
      warnings: [],
      meta: {
        editor: {
          kind: "rfiScan",
          titleKey: "rfi.fastScan.editor.title",
          rowCountKey: "rfi.fastScan.editor.rowCount",
          requiredFields: ["code"],
          columns: [
            {
              key: "code",
              labelKey: "rfi.fastScan.column.code",
              kind: "text",
            },
            {
              key: "target",
              labelKey: "rfi.fastScan.column.target",
              kind: "readonly",
            },
          ],
          rows: [
            {
              _rowId: "scan-output-0",
              code: "AZE-SUN-INT-QMS-RFI-CIV-0192",
              target:
                "/destination/CIV/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0192.pdf",
            },
          ],
        },
      },
    };
    const rows = (plan.meta.editor as Record<string, unknown>)
      .rows as Record<string, unknown>[];
    const onApply = vi.fn();
    const onUpdatePreview = vi.fn();
    render(
      <PlanView
        plan={plan}
        s={makeStrings("en")}
        error=""
        onCancel={vi.fn()}
        onApply={onApply}
        editorRows={rows}
        onEditorRowsChange={vi.fn()}
        editorDirty
        onUpdatePreview={onUpdatePreview}
      />,
    );
    fireEvent.keyDown(
      screen.getByRole("separator", {
        name: "Resize RFI number column",
      }),
      { key: "ArrowRight" },
    );
    expect(onApply).not.toHaveBeenCalled();

    const code = screen.getByRole("textbox", {
      name: "RFI number",
    });
    fireEvent.change(code, {
      target: { value: "AZE-SUN-INT-QMS-RFI-MEC-901" },
    });
    const update = screen.getByRole("button", {
      name: "Update preview",
    });
    expect(update).toBeEnabled();
    fireEvent.click(update);
    expect(onUpdatePreview).toHaveBeenCalledOnce();
    expect(onApply).not.toHaveBeenCalled();
  });

  it("shows a field error and disables preview for invalid scan codes", () => {
    const code = "AZE-SUN-INT-QMS-RFI-CLV-0192";
    const plan: Plan = {
      planId: "invalid-fast-scan-plan",
      opId: "rfi.process_scans_fast_covers",
      steps: [
        {
          kind: "copy",
          src: "/incoming/scan.pdf",
          dst: "/destination/RFI.pdf",
          noteKey: "note.renamed",
          meta: { code },
        },
      ],
      warnings: [],
      meta: {
        editor: {
          kind: "rfiScan",
          requiredFields: ["code"],
          columns: [
            {
              key: "code",
              labelKey: "rfi.fastScan.column.code",
              kind: "text",
            },
          ],
          rows: [{ _rowId: "scan-output-0", code }],
        },
      },
    };
    const editorProps = (candidate: string) => ({
      plan,
      s: makeStrings("en"),
      error: "",
      onCancel: vi.fn(),
      onApply: vi.fn(),
      editorRows: [{ _rowId: "scan-output-0", code: candidate }],
      onEditorRowsChange: vi.fn(),
      editorDirty: true,
      onUpdatePreview: vi.fn(),
    });
    const view = render(<PlanView {...editorProps(code)} />);

    for (const invalidCode of [
      code,
      "AZE-SUN-INT-QMS-RFI-CIV-901A",
      "AZE-SUN-INT-QMS-RFI-CIV-09010",
    ]) {
      view.rerender(<PlanView {...editorProps(invalidCode)} />);
      expect(
        screen.getByRole("textbox", { name: "RFI number" }),
      ).toHaveAttribute("aria-invalid", "true");
      expect(
        screen.getByRole("button", {
          name: "Update preview",
        }),
      ).toBeDisabled();
    }
  });

  it("explains missing concrete follow-up rows and names the skipped RFI", () => {
    const code = "AZE-SUN-INT-QMS-RFI-CIV-0124";
    render(
      <PlanWarnings
        warnings={[{ key: "warn.rfi.documents.followUpMissing", subject: code }]}
        focusableCodes={[]}
        s={makeStrings("en")}
      />,
    );

    expect(screen.getByText(code)).toBeVisible();
    expect(
      screen.getByText(/No Concrete Specimen Test Follow-Up row was found/),
    ).toBeVisible();
    expect(screen.getByText(/Apply creates available documents only/)).toBeVisible();
  });

  it("filters large warning sets and focuses only an exact related RFI", () => {
    const firstCode = "AZE-SUN-INT-QMS-RFI-MEC-0249";
    const secondCode = "AZE-SUN-INT-QMS-RFI-MEC-0250";
    const warnings: Plan["warnings"] = [
      { key: "warn.rfi.archiveMissing", subject: firstCode },
      { key: "warn.rfi.archiveAmbiguous", subject: secondCode },
      { key: "warn.rfi.reviewExists", subject: "review-a.pdf" },
      { key: "warn.rfi.reviewExists", subject: "review-b.pdf" },
      { key: "warn.rfi.signedUnreadable", subject: "combined.pdf — #4" },
      { key: "warn.rfi.signedUnreadable", subject: "combined.pdf — #8" },
    ];
    const onFocusCode = vi.fn();
    render(
      <PlanWarnings
        warnings={warnings}
        focusableCodes={[firstCode]}
        onFocusCode={onFocusCode}
        s={makeStrings("tr")}
      />,
    );

    expect(screen.getAllByRole("listitem")).toHaveLength(6);
    fireEvent.change(
      screen.getByRole("searchbox", { name: "Uyarılarda ara" }),
      {
        target: { value: "0249" },
      },
    );
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    fireEvent.click(
      screen.getByRole("button", { name: "İlgili sayfayı göster" }),
    );
    expect(onFocusCode).toHaveBeenCalledWith(firstCode);
    expect(screen.getByText("6 uyarının 1 tanesi gösteriliyor")).toBeVisible();

    expect(canonicalRfiInText(`issue: ${secondCode} — page 2`)).toBe(
      secondCode,
    );
    expect(canonicalRfiInText(`${secondCode}A`)).toBeNull();
    expect(canonicalRfiInText("AZE-SUN-INT-QMS-RFI-MEC-02Â49")).toBeNull();
  });

  it("derives exact existing-cell workbook comparisons only from approved step metadata", () => {
    const code = "AZE-SUN-INT-QMS-RFI-MEC-0249";
    const plan: Plan = {
      planId: "workbook-preview",
      opId: "rfi.system_sync",
      warnings: [],
      meta: {
        editor: {
          rows: [{ _rowId: "sync-row-1", canonicalRfi: code }],
        },
      },
      steps: [
        {
          kind: "workbook_update",
          src: "RFI Log.xlsm",
          dst: ".backup.xlsm",
          noteKey: "note.rfi.systemSync.rfiLog",
          meta: {
            syncStep: {
              workbookKind: "rfiLog",
              updates: {
                MEC: {
                  "249:20": {
                    rowId: "sync-row-1",
                    expectedValue: "Completed",
                    replacementValue: "Completed; Signed by Worley",
                    row: 249,
                    col: 20,
                  },
                },
              },
            },
          },
        },
        {
          kind: "workbook_update",
          src: "Inspection Status Follow Up.xlsm",
          dst: ".backup-follow-up.xlsm",
          noteKey: "note.rfi.followUp.workbookUpdate",
          meta: {
            updates: [
              {
                canonicalRfi: code,
                proposedRfi: "MEC-0248/0249",
                proposedStatus: "Signed",
                target: {
                  row: 42,
                  rfiColumn: 18,
                  statusColumn: 19,
                  worksheet: { name: "Signed By Worley MEC" },
                  rfiCell: { value: "MEC-0248" },
                  statusCell: { value: "Not Signed" },
                },
              },
            ],
          },
        },
        {
          kind: "workbook_update",
          src: "forged.xlsm",
          dst: "forged-backup.xlsm",
          noteKey: "forged",
          meta: {
            updates: [
              {
                canonicalRfi: `${code}A`,
                proposedStatus: "Signed",
                target: {
                  row: 0,
                  statusColumn: 99_999,
                  worksheet: { name: "Bad" },
                  statusCell: { value: "Unknown" },
                },
              },
            ],
          },
        },
      ],
    };

    expect(summarizeWorkbookCellChanges(plan)).toEqual([
      {
        workbook: "rfiLog",
        sheet: "MEC",
        cell: "T249",
        before: "Completed",
        after: "Completed; Signed by Worley",
        rfiCode: code,
      },
      {
        workbook: "followUp",
        sheet: "Signed By Worley MEC",
        cell: "R42",
        before: "MEC-0248",
        after: "MEC-0248/0249",
        rfiCode: code,
      },
      {
        workbook: "followUp",
        sheet: "Signed By Worley MEC",
        cell: "S42",
        before: "Not Signed",
        after: "Signed",
        rfiCode: code,
      },
    ]);
    render(
      <PlanView
        plan={plan}
        s={makeStrings("en")}
        error=""
        onCancel={vi.fn()}
        onApply={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("Exact workbook cell changes"));
    expect(screen.getByText("MEC!T249")).toBeVisible();
    expect(screen.getByText("Completed; Signed by Worley")).toBeVisible();
    expect(screen.getByText("Signed By Worley MEC!R42")).toBeVisible();
    expect(screen.getByText("MEC-0248/0249")).toBeVisible();
    expect(screen.getByText("Signed By Worley MEC!S42")).toBeVisible();
  });

  it("accepts only bounded PNG data thumbnails", () => {
    expect(safePngThumbnail("data:image/png;base64,iVBORw0KGgoAAA==")).toBe(
      "data:image/png;base64,iVBORw0KGgoAAA==",
    );
    expect(safePngThumbnail("https://example.test/page.png")).toBeNull();
    expect(safePngThumbnail("data:image/svg+xml;base64,AA==")).toBeNull();
    expect(safePngThumbnail("data:image/png;base64,<script>")).toBeNull();
    expect(safePngThumbnail("data:image/png;base64,c2lnbmVk")).toBeNull();
    expect(
      safePngThumbnail(`data:image/png;base64,${"A".repeat(350_000)}`),
    ).toBeNull();
    const largerValidPayload = `iVBORw0KGgo${"A".repeat(400_001)}`;
    expect(
      safePngThumbnail(`data:image/png;base64,${largerValidPayload}`),
    ).not.toBeNull();
    expect(
      safePngThumbnail(
        `data:image/png;base64,iVBORw0KGgo${"A".repeat(1_100_000)}`,
      ),
    ).toBeNull();
  });

  it("shows archive-root-relative candidates and enables apply only after selection", () => {
    const first = "Other/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf";
    const second = "Package/Civil/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf";
    const editor = {
      requiredFields: ["archiveFile"],
      columns: [
        { key: "enabled", labelKey: "rfi.signed.column.enabled", kind: "bool" },
        { key: "code", labelKey: "rfi.signed.column.code", kind: "readonly" },
        {
          key: "confidence",
          labelKey: "ocr.confidence",
          kind: "readonly",
          display: "percent",
        },
        {
          key: "archiveFile",
          labelKey: "rfi.signed.column.archive",
          kind: "choice",
        },
      ],
      choicesByValue: {
        archiveFile: {
          dependsOn: "code",
          values: {
            CODE: ["", "signed-CODE-candidate-1", "signed-CODE-candidate-2"],
          },
        },
      },
      choiceLabels: {
        archiveFile: {
          "": "rfi.signed.archive.choose",
          "signed-CODE-candidate-1": first,
          "signed-CODE-candidate-2": second,
        },
      },
    };
    const initialRows = [
      {
        _rowId: "signed-CODE",
        enabled: true,
        code: "CODE",
        confidence: 0.9,
        archiveFile: "",
        signedFile: "Incoming/combined-signed.pdf · page 1",
      },
    ];
    const plan: Plan = {
      planId: "plan-1",
      opId: "rfi.stage_signed_first_pages",
      steps: [
        {
          kind: "pdf_first_page_replace",
          src: "C:\\Signed\\combined.pdf",
          dst: "C:\\Review\\RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf",
          noteKey: "note.rfi.signedFirstPageStaged",
          meta: {},
        },
      ],
      warnings: [
        {
          key: "warn.rfi.archiveMissing",
          subject:
            "Incoming/unmatched.pdf · page 2 (#2) · AZE-SUN-INT-QMS-RFI-MEC-0999",
        },
      ],
      meta: {
        editor,
        pageComparisons: {
          "signed-CODE": {
            signed: "data:image/png;base64,iVBORw0KGgoAAA==",
            archives: {
              "signed-CODE-candidate-1":
                "data:image/png;base64,iVBORw0KGgoBBB==",
              "signed-CODE-candidate-2":
                "data:image/png;base64,iVBORw0KGgoCCC==",
            },
          },
        },
        pagePreviewOmitted: 0,
        warningReviewItems: [
          {
            sourceFile: "Incoming/unmatched.pdf",
            destination: "C:\\Review\\_review\\Incoming\\unmatched.pdf",
            status: "planned",
            reasons: [
              {
                key: "warn.rfi.archiveMissing",
                pageNumber: 2,
                code: "AZE-SUN-INT-QMS-RFI-MEC-0999",
                detail: "No archive PDF matched this exact RFI number",
              },
            ],
          },
        ],
      },
    };
    const onRowsChange = vi.fn();
    const props = {
      plan,
      s: makeStrings("tr"),
      error: "",
      onCancel: vi.fn(),
      onApply: vi.fn(),
      onEditorRowsChange: onRowsChange,
    };
    const view = render(<PlanView {...props} editorRows={initialRows} />);

    expect(screen.getByText("90%")).toBeVisible();
    const scrollRegion = view.container.querySelector(".plan-flow-scroll");
    expect(scrollRegion?.querySelector(".editor-card")).not.toBeNull();
    expect(scrollRegion?.querySelector(".action-bar")).toBeNull();
    expect(
      view.container.querySelector(".flow-layout > .action-bar"),
    ).not.toBeNull();
    expect(screen.getByText("Uygulamadan önce")).toBeVisible();
    expect(
      screen.getByText(
        "Henüz hiçbir şey değişmedi. Uygula yalnız bu onaylı önizlemeyi yürütür.",
      ),
    ).toBeVisible();
    fireEvent.click(screen.getByText("1 onaylı kesin etki"));
    expect(screen.getByText("C:\\Signed\\combined.pdf")).toBeVisible();
    expect(
      screen.getByText(
        "→ C:\\Review\\RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf",
      ),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Uygula" })).toBeDisabled();
    expect(screen.getByText("İlk sayfa karşılaştırması")).toBeVisible();
    expect(
      screen.getByRole("navigation", {
        name: "İmzalı sayfa önizleme bölümleri",
      }),
    ).toBeVisible();
    expect(screen.getByText("Onaylanan RFI'lar")).toBeVisible();
    expect(screen.getByText("Sayfaları karşılaştır")).toBeVisible();
    expect(screen.getByText("Review kopyaları")).toBeVisible();
    expect(screen.getByText("ÖNCE")).toBeVisible();
    expect(screen.getByText("SONRA")).toBeVisible();
    expect(
      screen.getByText(
        "Yalnızca 1. sayfa değiştirilecek; arşivdeki diğer sayfalar aynı kalacak.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText("Warning veren PDF'ler Review'a gönderilecek"),
    ).toBeVisible();
    expect(screen.getByText("Incoming/unmatched.pdf")).toBeVisible();
    expect(screen.getByText("Sayfa 2 · AZE-SUN-INT-QMS-RFI-MEC-0999")).toBeVisible();
    expect(
      screen.getByText("No archive PDF matched this exact RFI number"),
    ).toBeVisible();
    expect(screen.getByText("Kopyalanacak")).toBeVisible();
    expect(
      screen.getByText("İlk sayfasını göstermek için bir arşiv PDF'i seçin."),
    ).toBeVisible();
    expect(screen.getByAltText("CODE için yeni imzalı sayfa")).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgoAAA==",
    );
    const archiveBeforeModal = screen.getByRole("combobox");
    fireEvent.change(archiveBeforeModal, {
      target: { value: "signed-CODE-candidate-1" },
    });
    const editedBeforeModal = onRowsChange.mock.calls[
      onRowsChange.mock.calls.length - 1
    ][0] as Record<string, unknown>[];
    view.rerender(<PlanView {...props} editorRows={editedBeforeModal} />);
    expect(screen.getByRole("combobox")).toHaveValue("signed-CODE-candidate-1");
    fireEvent.click(
      screen.getByRole("button", {
        name: /CODE için yeni imzalı sayfa · Büyüt/,
      }),
    );
    expect(screen.getByRole("dialog")).toBeVisible();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(props.onCancel).not.toHaveBeenCalled();
    const archive = screen.getByRole("combobox");
    expect(archive).toHaveValue("signed-CODE-candidate-1");
    expect(within(archive).getByRole("option", { name: first })).toBeVisible();
    expect(within(archive).getByRole("option", { name: second })).toBeVisible();

    fireEvent.change(archive, { target: { value: "signed-CODE-candidate-2" } });
    const selectedRows = onRowsChange.mock.calls[
      onRowsChange.mock.calls.length - 1
    ][0] as Record<string, unknown>[];
    expect(selectedRows[0].archiveFile).toBe("signed-CODE-candidate-2");
    view.rerender(<PlanView {...props} editorRows={selectedRows} />);
    expect(screen.getByRole("button", { name: "Uygula" })).toBeEnabled();
    expect(
      screen.getByAltText("CODE için arşivdeki mevcut ilk sayfa"),
    ).toHaveAttribute("src", "data:image/png;base64,iVBORw0KGgoCCC==");
  });

  it("disables bool cells from row authority and localizes readonly key values", () => {
    const rows = [
      {
        _rowId: "one",
        selectable: false,
        enabled: false,
        reasonKey: "action.close",
      },
      {
        _rowId: "two",
        selectable: true,
        enabled: true,
        reasonKey: "action.close",
      },
    ];
    const plan: Plan = {
      planId: "plan-follow-up",
      opId: "rfi.follow_up_sync",
      steps: [],
      warnings: [],
      meta: {
        editor: {
          requiredFields: ["enabled"],
          columns: [
            {
              key: "enabled",
              labelKey: "rfi.signed.column.enabled",
              kind: "bool",
              disabledWhen: { field: "selectable", equals: false },
            },
            {
              key: "reasonKey",
              labelKey: "recovery.manual",
              kind: "readonly",
              display: "localizedKey",
            },
          ],
          rows,
        },
      },
    };
    render(
      <PlanView
        plan={plan}
        editorRows={rows}
        s={makeStrings("tr")}
        error=""
        onCancel={vi.fn()}
        onApply={vi.fn()}
        onEditorRowsChange={vi.fn()}
      />,
    );

    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes[0]).toBeDisabled();
    expect(checkboxes[1]).toBeEnabled();
    expect(screen.getAllByText("Kapat")).toHaveLength(2);
  });

  it("shows exact route and source context for Follow Up manual review rows", () => {
    const details =
      "ITP: CIV-0007; item: CIV-7.2.3; source context: tag=Camp; area=Camp; location=Camp; expected target: no configured Follow Up route; matching prepared target rows: none";
    const rows = [
      {
        _rowId: "unmapped-CIV-0042",
        rfiNo: "CIV-0042",
        reasonKey: "warn.rfi.followUp.reason.mappingUnsupported",
        mappingDetails: details,
        selectable: false,
        enabled: false,
      },
    ];
    const plan: Plan = {
      planId: "plan-follow-up-mapping-review",
      opId: "rfi.follow_up_sync",
      steps: [],
      warnings: [],
      meta: {
        editor: {
          requiredFields: ["enabled"],
          detailsEnabled: true,
          detailsDefaultVisible: true,
          columns: [
            {
              key: "reasonKey",
              labelKey: "rfi.followUp.column.reason",
              kind: "readonly",
              display: "localizedKey",
              detail: true,
            },
            {
              key: "mappingDetails",
              labelKey: "rfi.followUp.column.mappingDetails",
              kind: "readonly",
              detail: true,
              width: 320,
            },
          ],
          rows,
        },
      },
    };
    render(
      <PlanView
        plan={plan}
        editorRows={rows}
        s={makeStrings("tr")}
        error=""
        onCancel={vi.fn()}
        onApply={vi.fn()}
        onEditorRowsChange={vi.fn()}
      />,
    );

    expect(screen.getByText("Eşleme ayrıntıları")).toBeVisible();
    expect(screen.getByText(details)).toBeVisible();
    expect(
      screen.getByText(
        "Bu ITP/item birleşimi geçerli şablonda bir Follow Up rotasına sahip değil. Otomatik eşitlenemez.",
      ),
    ).toBeVisible();
  });

  it("renders the bounded Outlook reply as inert plain-text and escaped-HTML preview", () => {
    const plan: Plan = {
      planId: "plan-outlook",
      opId: "rfi.outlook_reply_draft",
      steps: [
        {
          kind: "outlook_reply_draft",
          src: "RFI response",
          dst: null,
          noteKey: "step.rfi.outlookReplyDraft",
          meta: { displayOnly: true, sendAllowed: false },
        },
      ],
      warnings: [],
      meta: {
        replyPreview: {
          subject: "RFI response",
          plainText: "Hello <script>alert(1)</script>",
          html: "<div>Hello &lt;script&gt;alert(1)&lt;/script&gt;</div><br><br>",
        },
      },
    };

    const view = render(
      <PlanView
        plan={plan}
        s={makeStrings("en")}
        error=""
        onCancel={vi.fn()}
        onApply={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("heading", { name: "Unsent Outlook reply preview" }),
    ).toBeVisible();
    const impact = view.container.querySelector<HTMLElement>(".plan-impact");
    expect(impact).not.toBeNull();
    expect(within(impact!).getByText("Drafts opened")).toBeVisible();
    expect(within(impact!).getByText("1")).toBeVisible();
    expect(screen.getByText("Hello <script>alert(1)</script>")).toBeVisible();
    expect(
      screen.getByText(
        "<div>Hello &lt;script&gt;alert(1)&lt;/script&gt;</div><br><br>",
      ),
    ).toBeVisible();
    expect(view.container.querySelector("script")).toBeNull();
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });

  it("requires account email and explains that username sign-in is unavailable", async () => {
    render(<App />);

    await screen.findByRole("heading", { name: "DocFlow" });
    expect(screen.getByLabelText("E-posta")).toBeVisible();
    expect(
      screen.getByText(
        "Kullanıcı adıyla giriş kullanılamıyor. Hesabının e-posta adresini kullan.",
      ),
    ).toBeVisible();
  });

  it("shows the exact runtime product version in the workspace", async () => {
    render(<App />);
    await signIn();

    expect(screen.getByText("Önizleme öncelikli")).toBeVisible();
    expect(screen.getByText(`v${__DOCFLOW_VERSION__}-demo`)).toBeVisible();
  });

  it("signs in and opens the schema-driven operation", async () => {
    render(<App />);
    await signIn();

    expect(
      await screen.findByRole("heading", { name: "İşlemler" }),
    ).toBeVisible();
    expect(
      screen.queryByText("Taramaları İşle", { selector: "strong" }),
    ).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByText("Hızlı RFI Taraması", { selector: "strong" }),
    );

    expect(
      await screen.findByRole("heading", { name: "Yapılandır" }),
    ).toBeVisible();
    expect(screen.getByText("Kaynak klasör")).toBeVisible();
    expect(screen.getByText("Hedef klasör")).toBeVisible();
  });

  it("hides page rotation in Single RFI mode and restores it in Multi RFI mode", async () => {
    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByText("Hızlı RFI Taraması", { selector: "strong" }),
    );
    await screen.findByRole("heading", { name: "Yapılandır" });

    const mode = screen.getByRole("combobox", { name: /Tarama modu/ });
    const rotationLabel = makeStrings("tr").t(
      "param.rfi.processScans.correctOrientation",
    );
    const rotation = () =>
      screen.queryByRole("switch", { name: rotationLabel });
    expect(rotation()).toBeInTheDocument();

    fireEvent.change(mode, { target: { value: "single_rfi" } });
    expect(rotation()).not.toBeInTheDocument();

    fireEvent.change(mode, { target: { value: "multi_rfi" } });
    expect(rotation()).toBeInTheDocument();
  });

  it("renders retired Process Scans history separately from unknown operation IDs", async () => {
    interceptMockBridge((method) =>
      method === "history/list"
        ? {
            handled: true,
            value: {
              online: true,
              rows: [
                {
                  when: "2026-08-24T00:00:00Z",
                  opId: "rfi.process_scans",
                  moves: 1,
                  failed: 0,
                  journalId: "legacy-scan",
                  steps: null,
                  cloudId: "legacy-scan",
                  undone: true,
                  undoAvailable: false,
                  fromCloud: true,
                },
                {
                  when: "2026-08-23T00:00:00Z",
                  opId: "future.unknown_operation",
                  moves: 1,
                  failed: 0,
                  journalId: "unknown-operation",
                  steps: null,
                  cloudId: "unknown-operation",
                  undone: true,
                  undoAvailable: false,
                  fromCloud: true,
                },
              ],
            },
          }
        : { handled: false },
    );
    render(<App />);
    await signIn();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "DocFlow" })).getByRole(
        "button",
        { name: "Geçmiş" },
      ),
    );

    expect(await screen.findAllByText("Taramaları İşle")).toHaveLength(1);
    expect(screen.getByText("Bilinmeyen işlem")).toBeVisible();
  });

  it.each([
    ["en", "Unknown operation"],
    ["tr", "Bilinmeyen işlem"],
    ["ru", "Неизвестная операция"],
    ["az", "Naməlum əməliyyat"],
  ] as const)("translates unknown history labels in %s", (locale, expected) => {
    expect(makeStrings(locale).t("history.unknownOperation")).toBe(expected);
  });

  it("keeps operations searchable and utilities reachable from persistent navigation", async () => {
    render(<App />);
    await signIn();

    const navigation = screen.getByRole("navigation", { name: "DocFlow" });
    for (const key of [
      "home.title",
      "action.review",
      "action.history",
      "recovery.title",
      "prefs.title",
      "action.admin",
    ] as const) {
      const label = makeStrings("tr").t(key);
      expect(
        within(navigation).getByRole("button", { name: label }),
      ).toBeVisible();
    }

    expect(
      screen.getByRole("button", {
        name: /Hızlı RFI Taraması/,
      }),
    ).toBeVisible();
    fireEvent.click(
      within(navigation).getByRole("button", {
        name: makeStrings("tr").t("action.history"),
      }),
    );
    expect(
      (await screen.findAllByRole("heading", { name: "Geçmiş" }))[0],
    ).toBeVisible();
  });


  it("opens a recovery center for pending journals and uncertain print queues", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      within(screen.getByRole("navigation", { name: "DocFlow" })).getByRole(
        "button",
        { name: "Kurtarma merkezi" },
      ),
    );

    expect(
      (await screen.findAllByRole("heading", { name: "Kurtarma merkezi" }))[0],
    ).toBeVisible();
    expect(
      await screen.findByText("Sonuç bilinmiyor — otomatik tekrar yok"),
    ).toBeVisible();
    expect(await screen.findByText("Belirsiz yazdırma kuyruğu")).toBeVisible();
  });

  it("offers explicit restore for a failed Follow Up Apply journal", async () => {
    let restored = false;
    const call = interceptMockBridge((method, params) => {
      if (method === "recovery/list") {
        return {
          handled: true,
          value: {
            journals: restored
              ? []
              : [
                  {
                    journalId: "failed-follow-up-apply",
                    disposition: "outcome_unknown",
                    restoreOriginalAvailable: true,
                  },
                ],
            printQueue: null,
          },
        };
      }
      if (method === "engine/undo") {
        restored = true;
        return {
          handled: true,
          value: {
            applied: 1,
            failed: 0,
            journalId: String(params.journalId),
            errors: [],
            meta: { effectState: "committed" },
            cloudSynced: null,
          },
        };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      within(screen.getByRole("navigation", { name: "DocFlow" })).getByRole(
        "button",
        { name: "Kurtarma merkezi" },
      ),
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Follow Up dosyasının aslını geri yükle",
      }),
    );

    await waitFor(() =>
      expect(call).toHaveBeenCalledWith("engine/undo", {
        journalId: "failed-follow-up-apply",
      }),
    );
    expect(await screen.findByText("Kurtarılacak öğe yok")).toBeVisible();
  });

  it("never offers undo for cloud history without a local journal", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      within(screen.getByRole("navigation", { name: "DocFlow" })).getByRole(
        "button",
        { name: "Geçmiş" },
      ),
    );

    await screen.findAllByRole("heading", { name: "Geçmiş" });
    expect(
      await screen.findAllByRole("button", { name: "Geri al" }),
    ).toHaveLength(1);
    expect(
      screen.getByText("Geri almak için bu cihazdaki yerel günlük gerekli."),
    ).toBeVisible();
  });

  it("renders the restored v0.5.4 RFI and print operation forms", async () => {
    render(<App />);
    await signIn();

    expect(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", {
        name: /İmzalı RFI ilk sayfalarını hazırla/,
      }),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    expect(await screen.findByText("RFI Log Excel dosyası")).toBeVisible();
    expect(screen.getByText(/\.xlsm.*\.xlsx/)).toBeVisible();
    expect(screen.getByText("Sheet'ler ve RFI adetleri")).toBeVisible();
    expect(screen.getByText("Talep mailini buraya yapıştır")).toBeVisible();
    expect(
      screen.queryByText("Gemini API anahtarı (yalnız Gemini için)"),
    ).toBeNull();

    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "gemini" },
    });
    expect(
      screen.getByText("Gemini API anahtarı (yalnız Gemini için)"),
    ).toBeVisible();
  });


  it("renders the signed first-page staging form from its schema", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", {
        name: /İmzalı RFI ilk sayfalarını hazırla/,
      }),
    );
    expect(
      await screen.findByText("Yeni imzalı taramalar klasörü"),
    ).toBeVisible();
    expect(screen.getByText("Mevcut RFI arşivinin kökü")).toBeVisible();
    expect(screen.getByText("Review çıktı klasörü")).toBeVisible();
    expect(
      screen.getByText("Yeni taramaların alt klasörlerini de tara"),
    ).toBeVisible();
  });

  it("bulk-edits RFI rows and keeps automatic fields behind a toggle", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    expect(
      await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx"),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "0008" })).toHaveAttribute(
      "title",
      "AZE-SUN-INT-QMS-RFI-CIV-0008",
    );
    const itpSelects = screen.getAllByRole("combobox", { name: "ITP" });
    expect(itpSelects[0]).toHaveValue("AZE-SUN-INT-QMS-ITP-CIV-0005");
    expect(
      within(itpSelects[0]).getByRole("option", { name: "5" }),
    ).toBeVisible();
    const itemSelects = screen.getAllByRole("combobox", { name: "Item" });
    expect(itemSelects[0]).toHaveValue("CIV-5.2.1");
    expect(
      within(itemSelects[0]).getByRole("option", { name: "5.2.1" }),
    ).toBeVisible();

    const issueDays = screen.getAllByLabelText("Veriliş — Gün");
    const issueMonths = screen.getAllByLabelText("Veriliş — Ay");
    expect(issueDays[0]).toHaveValue("15");
    expect(issueMonths[0]).toHaveValue("8");
    expect(issueDays[0].closest(".compact-date-cell")).toHaveAttribute(
      "title",
      "15-Aug-26",
    );
    expect(screen.queryByLabelText("Veriliş — Yıl")).toBeNull();
    fireEvent.click(screen.getAllByLabelText("Veriliş — Yılı değiştir")[0]);
    expect(screen.getByLabelText("Veriliş — Yıl")).toHaveValue("2026");

    fireEvent.change(screen.getByLabelText("Değiştirilecek alan"), {
      target: { value: "dateOfIssue" },
    });
    const bulkIssueDay = screen.getAllByLabelText("Veriliş — Gün")[0];
    const bulkIssueMonth = screen.getAllByLabelText("Veriliş — Ay")[0];
    fireEvent.change(bulkIssueDay, { target: { value: "20" } });
    fireEvent.change(bulkIssueMonth, { target: { value: "9" } });
    expect(bulkIssueDay.closest(".compact-date-cell")).toHaveAttribute(
      "title",
      `20-Sep-${String(new Date().getFullYear()).slice(-2)}`,
    );

    expect(
      screen.queryByRole("columnheader", { name: "Inspection Description" }),
    ).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "Otomatik alanları göster" }),
    );
    expect(
      screen.getByRole("columnheader", { name: "Inspection Description" }),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Tümünü seç" }));
    fireEvent.change(screen.getByLabelText("Değiştirilecek alan"), {
      target: { value: "subcontractor" },
    });
    fireEvent.change(screen.getByLabelText("Yeni değer"), {
      target: { value: "PDL" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Seçilenlere uygula" }));

    await waitFor(() => {
      const subcontractors = Array.from(
        document.querySelectorAll<HTMLInputElement>(
          'input[list^="editor-"][list$="-subcontractor"]',
        ),
        (input) => input.value,
      );
      expect(subcontractors).toEqual(["PDL", "PDL"]);
    });
  });

  it("selects a whole print group and includes or excludes it in one action", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("Yazdırma kuyruğunu kontrol et ve özelleştir"),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Tümünü seç" }));
    expect(screen.getByLabelText("Değiştirilecek alan")).toHaveValue("enabled");
    fireEvent.change(screen.getByLabelText("Yeni değer"), {
      target: { value: "false" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Seçilenlere uygula" }));

    for (const checkbox of screen.getAllByLabelText("Yazdır")) {
      expect(checkbox).not.toBeChecked();
    }
  });

  it("finds RFI documents and hands only type-selected opaque paths to Batch Print preview", async () => {
    const call = vi.spyOn(MockBridge.prototype, "call");
    render(<App />);
    await signIn();

    fireEvent.click(screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }));
    fireEvent.change(
      screen.getByRole("textbox", { name: /RFI numaraları/ }),
      {
        target: { value: "CIV-1319\nCIV-1320" },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "Gözat" }));
    await screen.findByDisplayValue("/Users/demo/Documents/DocFlow");
    fireEvent.click(screen.getByRole("button", { name: "Hiçbirini seçme" }));
    fireEvent.click(screen.getByLabelText("CIV-1319"));
    fireEvent.click(screen.getByRole("button", { name: "RFI Sisteminde ara" }));

    expect(await screen.findByText("AZE-SUN-INT-QMS-RFI-CIV-1319.docx")).toBeVisible();
    const firstPage = screen.getByRole("checkbox", {
      name: /AZE-SUN-INT-QMS-RFI-CIV-1319 İlk Sayfa/,
    });
    fireEvent.click(firstPage);
    expect(firstPage).not.toBeChecked();
    fireEvent.click(firstPage);
    expect(await screen.findByText("AZE-SUN-INT-QMS-RFI-CIV-1319.docx")).toBeVisible();
    expect(screen.queryByText("AZE-SUN-INT-QMS-RFI-CIV-1320.docx")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Sketch'leri temizle" }));
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));
    await screen.findByText("Yazdırma kuyruğunu kontrol et ve özelleştir");

    const previewCall = call.mock.calls.find(([method]) => method === "engine/preview");
    const previewParams = previewCall?.[1];
    if (
      !previewParams ||
      typeof previewParams !== "object" ||
      !("values" in previewParams) ||
      !previewParams.values ||
      typeof previewParams.values !== "object" ||
      !("files" in previewParams.values)
    ) {
      throw new Error("Batch Print preview values were not submitted");
    }
    const values = previewParams.values;
    expect(values.files).toMatchObject([
      { selectionId: "demo-rfi-print-0-0-0" },
    ]);
    expect(values).not.toHaveProperty("rfi_system_root");
    expect(values).not.toHaveProperty("rfi_numbers");
  });
  it("keeps bare-number cross-discipline candidates explicitly unselected", async () => {
    render(<App />);
    await signIn();
    fireEvent.click(screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }));
    fireEvent.change(
      screen.getByRole("textbox", { name: /RFI numaraları/ }),
      {
        target: { value: "1319" },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "Gözat" }));
    await screen.findByDisplayValue("/Users/demo/Documents/DocFlow");
    fireEvent.click(screen.getByRole("button", { name: "RFI Sisteminde ara" }));
    expect(await screen.findByText("AZE-SUN-INT-QMS-RFI-CIV-1319.docx")).toBeVisible();
    expect(screen.getByLabelText(/AZE-SUN-INT-QMS-RFI-CIV-1319 İlk Sayfa/)).not.toBeChecked();
    expect(screen.getByLabelText(/AZE-SUN-INT-QMS-RFI-MEC-1319 İlk Sayfa/)).not.toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: /CIV/ }));
    expect(screen.getByLabelText(/AZE-SUN-INT-QMS-RFI-CIV-1319 İlk Sayfa/)).toBeChecked();
    expect(screen.getByLabelText(/AZE-SUN-INT-QMS-RFI-MEC-1319 İlk Sayfa/)).not.toBeChecked();
  });

  it("submits one edited print page range without changing the other files or their order", async () => {
    const call = vi.spyOn(MockBridge.prototype, "call");

    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByRole("columnheader", { name: "Sayfa aralığı" }),
    ).toBeVisible();
    const pageRanges = screen.getAllByRole("textbox");
    expect(pageRanges).toHaveLength(2);
    expect(pageRanges[0]).toHaveValue("all");
    expect(pageRanges[1]).toHaveValue("all");

    fireEvent.change(pageRanges[0], { target: { value: "2-3" } });
    fireEvent.click(screen.getByRole("button", { name: "Uygula" }));

    await waitFor(() =>
      expect(
        call.mock.calls.filter(([method]) => method === "engine/revise"),
      ).toHaveLength(1),
    );
    const reviseCall = call.mock.calls.find(
      ([method]) => method === "engine/revise",
    );
    const rows = (reviseCall?.[1] as { rows: Record<string, unknown>[] }).rows;
    expect(
      rows.map(({ _rowId, file, path, pageRange }) => ({
        _rowId,
        file,
        path,
        pageRange,
      })),
    ).toStrictEqual([
      {
        _rowId: "file-1",
        file: "first.pdf",
        path: "/Users/demo/Documents/first.pdf",
        pageRange: "2-3",
      },
      {
        _rowId: "file-2",
        file: "second.docx",
        path: "/Users/demo/Documents/second.docx",
        pageRange: "all",
      },
    ]);
  });

  it("reorders worksheet rows before revise", async () => {
    const worksheetRows = [
      {
        _rowId: "worksheet-mec",
        enabled: false,
        status: "ready",
        file: "book.xlsx",
        path: "/Users/demo/Documents/book.xlsx",
        format: "XLSX",
        worksheetName: "MEC",
        worksheetVisibility: "visible",
        worksheet: {
          workbookIndex: 1,
          sheetId: 17,
          relationshipId: "rId1",
          relationshipType:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
          partPath: "xl/worksheets/sheet1.xml",
          name: "MEC",
          visibility: "visible",
        },
        signature: { size: 123, mtimeNs: 456, sha256: "a".repeat(64) },
        workbookSafety: {
          hasVbaProject: false,
          hasXlmMacroSheet: false,
          hasWorkbookProtection: false,
          externalLinkCount: 0,
          connectionCount: 0,
          queryTableCount: 0,
        },
        pageRange: "all",
        printer: "Office Printer",
        paper: "A4",
        orientation: "auto",
        scaling: "fit",
        color: "printer",
        duplex: "printer",
        copies: 1,
      },
      {
        _rowId: "worksheet-hidden",
        enabled: false,
        status: "ready",
        file: "book.xlsx",
        path: "/Users/demo/Documents/book.xlsx",
        format: "XLSX",
        worksheetName: "Hidden",
        worksheetVisibility: "hidden",
        worksheet: {
          workbookIndex: 2,
          sheetId: 44,
          relationshipId: "rId2",
          relationshipType:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
          partPath: "xl/worksheets/sheet2.xml",
          name: "Hidden",
          visibility: "hidden",
        },
        signature: { size: 123, mtimeNs: 456, sha256: "a".repeat(64) },
        workbookSafety: {
          hasVbaProject: false,
          hasXlmMacroSheet: false,
          hasWorkbookProtection: false,
          externalLinkCount: 0,
          connectionCount: 0,
          queryTableCount: 0,
        },
        pageRange: "all",
        printer: "Office Printer",
        paper: "A4",
        orientation: "auto",
        scaling: "fit",
        color: "printer",
        duplex: "printer",
        copies: 1,
      },
      {
        _rowId: "worksheet-very-hidden",
        enabled: false,
        status: "ready",
        file: "book.xlsx",
        path: "/Users/demo/Documents/book.xlsx",
        format: "XLSX",
        worksheetName: "VeryHidden",
        worksheetVisibility: "veryHidden",
        worksheet: {
          workbookIndex: 3,
          sheetId: 45,
          relationshipId: "rId3",
          relationshipType:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
          partPath: "xl/worksheets/sheet3.xml",
          name: "VeryHidden",
          visibility: "veryHidden",
        },
        signature: { size: 123, mtimeNs: 456, sha256: "a".repeat(64) },
        workbookSafety: {
          hasVbaProject: false,
          hasXlmMacroSheet: false,
          hasWorkbookProtection: false,
          externalLinkCount: 0,
          connectionCount: 0,
          queryTableCount: 0,
        },
        pageRange: "all",
        printer: "Office Printer",
        paper: "A4",
        orientation: "auto",
        scaling: "fit",
        color: "printer",
        duplex: "printer",
        copies: 1,
      },
    ];
    const editor = {
      kind: "recordTable",
      titleKey: "print.editor.title",
      rowCountKey: "print.editor.rowCount",
      reorderable: true,
      columns: [
        { key: "enabled", labelKey: "print.col.include", kind: "bool" },
        {
          key: "worksheetName",
          labelKey: "print.col.worksheet",
          kind: "readonly",
        },
        {
          key: "worksheetVisibility",
          labelKey: "print.col.worksheetVisibility",
          kind: "readonly",
        },
      ],
      rows: worksheetRows,
    };
    const worksheetPlan: Plan = {
      planId: "worksheet-plan",
      opId: "files.batch_print",
      warnings: [],
      meta: { editor },
      steps: [],
    };
    const call = interceptMockBridge((method, params) => {
      if (method === "engine/preview") {
        return { handled: true, value: worksheetPlan };
      }
      if (method === "engine/revise") {
        return {
          handled: true,
          value: {
            ...worksheetPlan,
            planId: "worksheet-plan-revised",
            meta: {
              editor: {
                ...editor,
                rows: params.rows,
              },
            },
          },
        };
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
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByRole("columnheader", { name: "Çalışma sayfası" }),
    ).toBeVisible();
    expect(
      screen.getByRole("columnheader", { name: "Görünürlük" }),
    ).toBeVisible();
    const hiddenRow = (await screen.findByText("Hidden")).closest("tr");
    expect(hiddenRow).not.toBeNull();
    expect(within(hiddenRow!).getByText("Gizli")).toBeVisible();
    const apply = screen.getByRole("button", { name: "Uygula" });
    expect(apply).toBeDisabled();
    fireEvent.click(
      within(hiddenRow!).getByRole("checkbox", { name: "Yazdır" }),
    );
    expect(apply).toBeEnabled();
    fireEvent.click(
      within(hiddenRow!).getByRole("button", { name: "Yukarı taşı" }),
    );
    fireEvent.click(apply);

    await waitFor(() =>
      expect(
        call.mock.calls.filter(([method]) => method === "engine/revise"),
      ).toHaveLength(1),
    );
    const reviseCall = call.mock.calls.find(
      ([method]) => method === "engine/revise",
    );
    const rows = (reviseCall?.[1] as { rows: Record<string, unknown>[] }).rows;
    expect(rows.map((row) => row._rowId)).toStrictEqual([
      "worksheet-hidden",
      "worksheet-mec",
      "worksheet-very-hidden",
    ]);
    expect(rows).toStrictEqual([
      { ...worksheetRows[1], enabled: true },
      worksheetRows[0],
      worksheetRows[2],
    ]);
  });

  it("opens preferences before authentication", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "DocFlow" });
    fireEvent.click(screen.getByRole("button", { name: "Ayarlar" }));
    expect(
      await screen.findByRole("heading", { name: "Ayarlar" }),
    ).toBeVisible();
    expect(screen.getByText("Dil")).toBeVisible();
    expect(screen.getByText("Tema")).toBeVisible();
    expect(
      screen.getByText("Windows açılışında DocFlow'u başlat"),
    ).toBeVisible();
    expect(screen.getByText("Onaysız RFI Sistem sync aktif")).toBeVisible();
  });

  it("previews and explicitly copies a local redacted support bundle without sending it", async () => {
    const content = `${JSON.stringify(
      {
        schemaVersion: 1,
        docflow: { version: "0.6.0" },
        platform: { arch: "aarch64", os: "macos" },
        components: {
          ocr: {
            engine: "tesseract",
            languageDataAvailable: true,
            status: "available",
          },
          sidecar: { required: false, status: "retired" },
        },
        recentErrorCodes: ["validation"],
        privacy: { delivery: "local-only", redaction: "recursive" },
      },
      null,
      2,
    )}\n`;
    const call = interceptMockBridge((method) =>
      method === "support/diagnostics"
        ? {
            handled: true,
            value: {
              fileName: "docflow-support-diagnostics-v1.json",
              mediaType: "application/json",
              byteLength: content.length,
              content,
            },
          }
        : method === "support/copyDiagnostics"
          ? { handled: true, value: { ok: true } }
          : { handled: false },
    );

    render(<App />);
    await screen.findByRole("heading", { name: "DocFlow" });
    fireEvent.click(screen.getByRole("button", { name: "Ayarlar" }));
    await screen.findByRole("heading", { name: "Ayarlar" });

    expect(screen.getByText(/hiçbir yere gönderilmez/i)).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: "Destek paketini önizle" }),
    );

    const preview = await screen.findByLabelText("Destek paketi önizlemesi");
    expect(preview).toHaveTextContent('"version": "0.6.0"');
    expect(preview).toHaveTextContent('"status": "retired"');
    expect(preview).toHaveTextContent('"recentErrorCodes"');
    expect(
      call.mock.calls.filter(([method]) => method === "support/diagnostics"),
    ).toHaveLength(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Paketi panoya kopyala" }),
    );
    await screen.findByText("Destek paketi panoya kopyalandı.");
    expect(call).toHaveBeenCalledWith("support/copyDiagnostics", { content });
    expect(
      call.mock.calls.some(([method]) => /upload|send|telemetry/i.test(method)),
    ).toBe(false);
  });

  it("starts print clean and restores only previous safe settings on request", async () => {
    const savedParams: Record<string, unknown>[] = [];
    interceptMockBridge((method, params) => {
      if (method === "settings/params") {
        return {
          handled: true,
          value: {
            files: ["C:/stale/old-job.pdf"],
            paper: "A3",
            copies: 9,
            duplex: "long",
          },
        };
      }
      if (method === "settings/saveParams") {
        savedParams.push(
          (params as { values: Record<string, unknown> }).values,
        );
        return { handled: true, value: { ok: true } };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    );

    const copiesInput = await screen.findByRole("spinbutton", {
      name: /^Kopya sayısı/,
    });
    expect(copiesInput).toHaveValue(1);
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A4");
    expect(screen.queryByText("old-job.pdf")).toBeNull();

    fireEvent.click(
      await screen.findByRole("button", { name: "Önceki ayarları getir" }),
    );
    expect(copiesInput).toHaveValue(9);
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue("A3");
    expect(screen.getByRole("combobox", { name: /^Çift taraflı/ })).toHaveValue(
      "long",
    );

    fireEvent.change(copiesInput, { target: { value: "7" } });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 250));
    });
    expect(savedParams).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "Dosya ekle" }));
    await screen.findByText("first.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    await waitFor(() => {
      expect(savedParams).toHaveLength(1);
    });
    expect(savedParams[0]).toMatchObject({ copies: 7, paper: "A3" });
    expect(savedParams[0]).not.toHaveProperty("files");
  });

  it("filters and searches rows in the preview editor table", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx");
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();

    const searchInput = screen.getByLabelText(
      "Ara (RFI no, durum, açıklama)...",
    );
    expect(searchInput).toBeVisible();

    // Search for 0008
    fireEvent.change(searchInput, { target: { value: "0008" } });
    expect(screen.getByRole("button", { name: "0008" })).toBeVisible();

    // Filter by changes tab
    const changesTab = screen.getByRole("button", { name: "Değişiklikler" });
    fireEvent.click(changesTab);
    expect(changesTab).toHaveClass("filter-tab--active");

    // Clear search
    fireEvent.change(searchInput, { target: { value: "" } });
    const allTab = screen.getByRole("button", { name: "Tümü" });
    fireEvent.click(allTab);
    expect(allTab).toHaveClass("filter-tab--active");
  });

  it("supports keyboard shortcuts Ctrl+Enter and Escape", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx");

    // Press Ctrl+Enter to trigger preview
    fireEvent.keyDown(window, { key: "Enter", ctrlKey: true });

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();

    // Press Escape to return to form
    fireEvent.keyDown(window, { key: "Escape" });

    expect(await screen.findByRole("button", { name: "Önizle" })).toBeVisible();
  });

  it("supports quick theme toggling via header button and Alt+T shortcut", async () => {
    let currentTheme = "system";
    interceptMockBridge((method, params) => {
      if (method === "settings/update") {
        currentTheme = (params.theme as string) || currentTheme;
        return {
          handled: true,
          value: {
            locale: null,
            theme: currentTheme,
            font: "system",
            fontScale: 1.0,
            compact: false,
          },
        };
      }
      return { handled: false };
    });

    render(<App />);
    await signIn();

    const toggleBtn = screen.getByRole("button", {
      name: /Temayı değiştir/i,
    });
    expect(toggleBtn).toBeVisible();

    // Initial is system
    expect(document.documentElement.dataset.theme).toBe("system");

    // Click toggle: system -> light
    fireEvent.click(toggleBtn);
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("light");
    });

    // Press Alt+T: light -> dark
    fireEvent.keyDown(window, { key: "t", altKey: true });
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("dark");
    });
  });

  it("shows quick clear button for browse path fields and clears on click", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /RFI Günlüğüne Çoklu Kayıt Ekle/ }),
    );
    const browseBtn = await screen.findByRole("button", { name: "Gözat" });
    fireEvent.click(browseBtn);

    // Verify clear button appears
    const clearBtn = await screen.findByRole("button", { name: "Temizle" });
    expect(clearBtn).toBeVisible();

    // Click clear
    fireEvent.click(clearBtn);

    // Verify clear button disappears and placeholder returns
    await waitFor(() => {
      expect(
        screen.queryByRole("button", { name: "Temizle" }),
      ).not.toBeInTheDocument();
    });
  });

  it("filters operations by search text and category pills on home screen", async () => {
    render(<App />);
    await signIn();

    const searchInput = screen.getByPlaceholderText("İşlem ara…");
    expect(searchInput).toBeVisible();

    fireEvent.change(searchInput, { target: { value: "Yazdır" } });
    expect(
      screen.getByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: "" } });
    expect(
      screen.getByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).toBeVisible();

    const chipsContainer = document.querySelector(".category-filter-chips")!;
    const scansChip = within(chipsContainer as HTMLElement).getByRole("button", {
      name: /Taramalar ve PDF/,
    });
    fireEvent.click(scansChip);
    expect(scansChip).toHaveClass("active");
    expect(
      screen.getByRole("button", { name: /^Hızlı RFI Taraması/ }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Birden fazla dosya yazdır/ }),
    ).not.toBeInTheDocument();
  });
});
