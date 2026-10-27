import { describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { makeStrings } from "./i18n";
import { PlanView } from "./plan/PlanView";
import type { Plan } from "./types";
import { registerAppTestCleanup } from "./test/appTestSupport";

registerAppTestCleanup();

describe("DocFlow shell: workbook candidates and manual review", () => {
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
    expect(screen.getByText("İnceleme kopyaları")).toBeVisible();
    expect(screen.getByText("ÖNCE")).toBeVisible();
    expect(screen.getByText("SONRA")).toBeVisible();
    expect(
      screen.getByText(
        "Yalnızca 1. sayfa değiştirilecek; arşivdeki diğer sayfalar aynı kalacak.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText("Uyarı veren PDF'ler İnceleme'a gönderilecek"),
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
});
