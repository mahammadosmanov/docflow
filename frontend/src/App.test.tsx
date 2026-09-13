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

import { App, humanError, isPlanEditorReady, PlanView } from "./App";
import { makeStrings } from "./i18n";
import { MockBridge, resetBridgeForTests } from "./lib/bridge";
import type { Plan } from "./types";

function interceptMockBridge(
  intercept: (
    method: string,
    params: Record<string, unknown>,
  ) => { handled: boolean; value?: unknown },
) {
  const originalCall = MockBridge.prototype.call;
  return vi
    .spyOn(MockBridge.prototype, "call")
    .mockImplementation(function (this: MockBridge, method, ...args) {
      const result = intercept(
        method,
        (args[0] ?? {}) as Record<string, unknown>,
      );
      if (result.handled) return Promise.resolve(result.value) as never;
      return Reflect.apply(originalCall, this, [method, ...args]);
    });
}

async function signIn() {
  expect(
    await screen.findByRole("heading", { name: "DocFlow" }),
  ).toBeVisible();
  fireEvent.change(
    screen.getByLabelText("E-posta"),
    { target: { value: "demo@example.com" } },
  );
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

describe("DocFlow shell", () => {
  it("shows device readiness and routes attention to recovery", async () => {
    const call = interceptMockBridge((method) =>
      method === "support/readiness"
        ? {
            handled: true,
            value: {
              overall: "attention",
              checks: [
                { id: "ocr", status: "ready", count: 1 },
                { id: "printers", status: "ready", count: 2 },
                { id: "recovery", status: "attention", count: 1 },
              ],
            },
          }
        : { handled: false },
    );

    render(<App />);
    await signIn();

    expect(await screen.findByText("Cihaz hazırlığı")).toBeVisible();
    expect(screen.getByText("Dikkat gerekiyor")).toBeVisible();
    expect(screen.getByText("2 yazıcı kullanılabilir")).toBeVisible();
    expect(call).toHaveBeenCalledWith("support/readiness");

    fireEvent.click(screen.getByRole("button", { name: "Kurtarmayı aç" }));
    expect(
      (await screen.findAllByRole("heading", { name: "Kurtarma merkezi" }))[0],
    ).toBeVisible();
  });

  it("operation presets load only for capability-enabled operations", async () => {
    const call = vi.spyOn(MockBridge.prototype, "call");

    render(<App />);
    await signIn();
    fireEvent.click(
      screen.getByRole("button", { name: /Taramaları İşle/ }),
    );
    await screen.findByRole("heading", { name: "Ayarlar" });
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
    fireEvent.click(
      screen.getByRole("button", { name: "Ön ayarı uygula" }),
    );
    expect(
      screen.getByRole("combobox", { name: /^Yazıcı/ }),
    ).toHaveValue("Disconnected Printer");
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
    fireEvent.click(
      screen.getByRole("button", { name: "Ön ayarı uygula" }),
    );
    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue(
      "A3",
    );
    fireEvent.change(
      screen.getByRole("spinbutton", { name: /^Kopya sayısı/ }),
      { target: { value: "7" } },
    );

    await act(async () => {
      resolveParams({ paper: "A4", copies: 2 });
      await paramsResult;
    });

    expect(screen.getByRole("combobox", { name: /^Kağıt/ })).toHaveValue(
      "A3",
    );
    expect(
      screen.getByRole("spinbutton", { name: /^Kopya sayısı/ }),
    ).toHaveValue(7);
  });

  it("operation presets save only current bool choice and int values", async () => {
    let presets: unknown[] = [];
    let savedParams: Record<string, unknown> | null = null;
    const call = interceptMockBridge((method, params) => {
      if (method === "settings/presets") {
        return { handled: true, value: presets };
      }
      if (method === "settings/savePreset") {
        savedParams = params;
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
    expect(savedParams).not.toBeNull();
    expect(savedParams).toStrictEqual({
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
    fireEvent.change(
      await screen.findByLabelText("Yeni ön ayar adı"),
      { target: { value: "A4 standard" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Ön ayarı kaydet" }),
    );

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
    fireEvent.click(
      screen.getByRole("button", { name: "Ön ayarı sil" }),
    );

    expect(confirm).toHaveBeenCalledWith(
      "A3 duplex adlı ön ayar silinsin mi?",
    );
    expect(screen.getByRole("option", { name: "A3 duplex" })).toBeVisible();

    resolveDelete();
    await waitFor(() =>
      expect(
        screen.queryByRole("option", { name: "A3 duplex" }),
      ).toBeNull(),
    );
    expect(
      screen.getByRole("option", { name: "A4 simplex" }),
    ).toBeVisible();
    expect(
      call.mock.calls.filter(([method]) => method === "settings/presets"),
    ).toHaveLength(2);
  });

  it("operation presets show localized command errors", async () => {
    const writeError = Object.assign(
      new Error("settings could not be saved"),
      { code: "settings_write_failed" },
    );
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
    fireEvent.change(
      await screen.findByLabelText("Yeni ön ayar adı"),
      { target: { value: "Unavailable" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Ön ayarı kaydet" }),
    );

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

  it("shows archive-root-relative candidates and enables apply only after selection", () => {
    const first = "Other/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf";
    const second = "Package/Civil/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0210.pdf";
    const editor = {
      requiredFields: ["archiveFile"],
      columns: [
        { key: "enabled", labelKey: "rfi.signed.column.enabled", kind: "bool" },
        { key: "code", labelKey: "rfi.signed.column.code", kind: "readonly" },
        { key: "archiveFile", labelKey: "rfi.signed.column.archive", kind: "choice" },
      ],
      choicesByValue: {
        archiveFile: {
          dependsOn: "code",
          values: { CODE: ["", "signed-CODE-candidate-1", "signed-CODE-candidate-2"] },
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
      { _rowId: "signed-CODE", enabled: true, code: "CODE", archiveFile: "" },
    ];
    const plan: Plan = {
      planId: "plan-1",
      opId: "rfi.stage_signed_first_pages",
      steps: [],
      warnings: [],
      meta: { editor },
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

    expect(screen.getByRole("button", { name: "Uygula" })).toBeDisabled();
    const archive = screen.getByRole("combobox");
    expect(within(archive).getByRole("option", { name: first })).toBeVisible();
    expect(within(archive).getByRole("option", { name: second })).toBeVisible();

    fireEvent.change(archive, { target: { value: "signed-CODE-candidate-2" } });
    const selectedRows = onRowsChange.mock.calls[0][0] as Record<string, unknown>[];
    expect(selectedRows[0].archiveFile).toBe("signed-CODE-candidate-2");
    view.rerender(<PlanView {...props} editorRows={selectedRows} />);
    expect(screen.getByRole("button", { name: "Uygula" })).toBeEnabled();
  });

  it("disables bool cells from row authority and localizes readonly key values", () => {
    const rows = [
      { _rowId: "one", selectable: false, enabled: false, reasonKey: "action.close" },
      { _rowId: "two", selectable: true, enabled: true, reasonKey: "action.close" },
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

    expect(screen.getByRole("heading", { name: "Unsent Outlook reply preview" })).toBeVisible();
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

    expect(screen.getByText("v0.8.0-demo")).toBeVisible();
  });

  it("signs in and opens the schema-driven operation", async () => {
    render(<App />);
    await signIn();

    expect(
      await screen.findByRole("heading", { name: "İşlemler" }),
    ).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: /Taramaları İşle/ }),
    );

    expect(
      await screen.findByRole("heading", { name: "Ayarlar" }),
    ).toBeVisible();
    expect(screen.getByText("Kaynak klasör")).toBeVisible();
    expect(screen.getByText("Hedef klasör")).toBeVisible();
  });

  it("exposes every preserved feature from the workspace", async () => {
    render(<App />);
    await signIn();

    expect(screen.getByText("Diğer özellikler")).toBeVisible();
    expect(
      screen.getByText("Bekleyen dosyaları tek tek kontrol et ve yerleştir."),
    ).toBeVisible();
    expect(
      screen.getByText("Yapılan işlemleri gör ve güvenle geri al."),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Dil, görünüm, yazı ve hesap tercihlerini yönet.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Kullanıcı erişimlerini ve yönetici yetkilerini yönet.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Bekleyen günlükleri ve belirsiz yazdırma işlerini güvenle incele.",
      ),
    ).toBeVisible();

    fireEvent.click(
      screen
        .getByText("Yapılan işlemleri gör ve güvenle geri al.")
        .closest("button")!,
    );
    const historyHeadings = await screen.findAllByRole("heading", {
      name: "Geçmiş",
    });
    expect(historyHeadings[0]).toBeVisible();
  });

  it("opens a recovery center for pending journals and uncertain print queues", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen
        .getByText(
          "Bekleyen günlükleri ve belirsiz yazdırma işlerini güvenle incele.",
        )
        .closest("button")!,
    );

    expect(
      (await screen.findAllByRole("heading", { name: "Kurtarma merkezi" }))[0],
    ).toBeVisible();
    expect(
      await screen.findByText("Sonuç bilinmiyor — otomatik tekrar yok"),
    ).toBeVisible();
    expect(await screen.findByText("Belirsiz yazdırma kuyruğu")).toBeVisible();
  });

  it("never offers undo for cloud history without a local journal", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen
        .getByText("Yapılan işlemleri gör ve güvenle geri al.")
        .closest("button")!,
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
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
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
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
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
    expect(await screen.findByText("Yeni imzalı taramalar klasörü")).toBeVisible();
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
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "0008" })).toHaveAttribute(
      "title",
      "AZE-SUN-INT-QMS-RFI-CIV-0008",
    );
    const itpSelects = screen.getAllByLabelText("ITP");
    expect(itpSelects[0]).toHaveValue("AZE-SUN-INT-QMS-ITP-CIV-0005");
    expect(
      within(itpSelects[0]).getByRole("option", { name: "5" }),
    ).toBeVisible();
    const itemSelects = screen.getAllByLabelText("Item");
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
    fireEvent.click(
      screen.getByRole("button", { name: "Seçilenlere uygula" }),
    );

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
    fireEvent.click(
      screen.getByRole("button", { name: "Seçilenlere uygula" }),
    );

    for (const checkbox of screen.getAllByLabelText("Yazdır")) {
      expect(checkbox).not.toBeChecked();
    }
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
        { key: "worksheetName", labelKey: "print.col.worksheet", kind: "readonly" },
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
    fireEvent.click(within(hiddenRow!).getByRole("button", { name: "Yukarı taşı" }));
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

    expect(
      screen.getByText(/hiçbir yere gönderilmez/i),
    ).toBeVisible();
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

  it("auto-saves parameters on change and reuses saved parameters", async () => {
    let savedParams: Record<string, unknown> | null = null;
    interceptMockBridge((method, params) => {
      if (method === "settings/saveParams") {
        savedParams = (params as { values: Record<string, unknown> }).values;
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
    fireEvent.change(copiesInput, { target: { value: "9" } });

    await waitFor(() => {
      expect(savedParams).toMatchObject({ copies: 9 });
    });
  });

  it("filters and searches rows in the preview editor table", async () => {
    render(<App />);
    await signIn();

    fireEvent.click(
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();

    const searchInput = screen.getByLabelText("Ara (RFI no, durum, açıklama)...");
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
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Gözat" }));

    // Press Ctrl+Enter to trigger preview
    fireEvent.keyDown(window, { key: "Enter", ctrlKey: true });

    expect(
      await screen.findByText("RFI kayıtlarını kontrol et ve düzenle"),
    ).toBeVisible();

    // Press Escape to return to form
    fireEvent.keyDown(window, { key: "Escape" });

    expect(
      await screen.findByRole("button", { name: "Önizle" }),
    ).toBeVisible();
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
      screen.getByRole("button", { name: /Çoklu sheet RFI Log kaydı ekle/ }),
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
      expect(screen.queryByRole("button", { name: "Temizle" })).not.toBeInTheDocument();
    });
  });
});
