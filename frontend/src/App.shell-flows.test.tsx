import { describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { App } from "./App";
import { makeStrings } from "./i18n";
import { MockBridge } from "./lib/bridge";
import type { Plan } from "./types";
import { interceptMockBridge, signIn, registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: sign-in, navigation, recovery and forms", () => {
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
        name: /^Hızlı RFI Taraması/,
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
    expect(screen.getByText("Sayfalar ve RFI adetleri")).toBeVisible();
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
    expect(screen.getByText("İnceleme çıktı klasörü")).toBeVisible();
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
    fireEvent.click(await screen.findByRole("button", { name: /^Gözat/ }));
    expect(
      await screen.findByDisplayValue("/Users/demo/Documents/RFI Log.xlsx"),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Önizle" }));

    expect(
      await screen.findByText("RFI kayıtlarını kontrol edin ve düzenleyin"),
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
    const itemSelects = screen.getAllByRole("combobox", { name: "Madde No" });
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
      screen.queryByRole("columnheader", { name: "Açıklama" }),
    ).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "Otomatik alanları göster" }),
    );
    expect(
      screen.getByRole("columnheader", { name: "Açıklama" }),
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
      await screen.findByText("Yazdırma kuyruğunu kontrol edin ve özelleştirin"),
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
    fireEvent.click(screen.getByRole("button", { name: /^Gözat/ }));
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
    await screen.findByText("Yazdırma kuyruğunu kontrol edin ve özelleştirin");

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
    fireEvent.click(screen.getByRole("button", { name: /^Gözat/ }));
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
});
