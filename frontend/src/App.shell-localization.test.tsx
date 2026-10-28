import { describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { makeStrings } from "./i18n";
import { humanError } from "./helpers";
import { isPlanEditorReady } from "./plan/EditablePlanTable";
import { PlanView } from "./plan/PlanView";
import {
  canonicalRfiInText,
  summarizeWorkbookCellChanges,
} from "./plan/PlanSummaries";
import { PlanWarnings } from "./plan/PlanWarnings";
import { safePngThumbnail } from "./plan/SignedPageComparison";
import { MockBridge } from "./lib/bridge";
import type { Plan } from "./types";
import { registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: localization and scan preview", () => {
  it("localizes the invalid RFI workbook error in every supported language", () => {
    const expected = {
      tr: "Geçerli bir makro etkin .xlsm veya standart .xlsx RFI Log dosyası seçin.",
      en: "Select a valid macro-enabled .xlsm or standard .xlsx RFI Log workbook.",
      az: "Etibarlı makro aktiv .xlsm və ya standart .xlsx RFI Log faylı seçin.",
      ru: "Выберите допустимый RFI Log с поддержкой макросов .xlsm или стандартный .xlsx.",
    } as const;
    const followUpExpected = {
      tr: "Geçerli RFI Log ve Follow Up .xlsm veya .xlsx çalışma kitapları seçin.",
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
      tr: "Bu eski yazdırma kuyruğu güvenle devam ettirilemez. Kuyruğu silip dosyaları yeniden seçin.",
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
});
