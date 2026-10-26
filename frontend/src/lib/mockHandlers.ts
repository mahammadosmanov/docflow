import { BridgeError, type ProgressHandler } from "./bridge";
import { demoFastCoverOperation } from "./demoOperations";
import {
  demoOperationList,
  isEmptyValue,
  mockPath,
  normalizeMockRfiCode,
} from "./mockFixtures";
import {
  type Plan,
  type RfiPrintFindResult,
  type SupportBundlePreview,
} from "../types";

export type MockPlanState = {
  printPlan: Plan | null;
  fileGroupPlan: Plan | null;
  completeStatusPlan: Plan | null;
  fastScanPlan: Plan | null;
};

export function printFindOperation(
  params: Record<string, unknown>,
  onProgress?: ProgressHandler,
): unknown {
  let result: unknown;
  const requests = Array.isArray(params.numbers) ? (params.numbers as string[]) : [];
  const candidates = requests.flatMap((requested, requestIndex) => {
    const normalized = requested.toUpperCase();
    const codes = /^\d{4}$/.test(normalized)
      ? ["CIV", "MEC"].map((discipline) => `AZE-SUN-INT-QMS-RFI-${discipline}-${normalized}`)
      : [
          normalized.startsWith("AZE-SUN-INT-QMS-RFI-")
            ? normalized
            : `AZE-SUN-INT-QMS-RFI-${normalized}`,
        ];
    const types: Array<"firstPage" | "sketch"> = [];
    if (params.firstPage) types.push("firstPage");
    if (params.sketch) types.push("sketch");
    return codes.flatMap((code, codeIndex) =>
      types.map((documentType, typeIndex) => {
        const extension = documentType === "firstPage" ? ".docx" : " Sketch.pdf";
        const displayPath = `${code}${extension}`;
        return {
          requested,
          rfiCode: code,
          documentType,
          displayPath,
          selection: {
            selectionId: `demo-rfi-print-${requestIndex}-${codeIndex}-${typeIndex}`,
            displayPath: `/Users/demo/Documents/DocFlow/${displayPath}`,
            name: displayPath,
          },
          selected: codes.length === 1,
        };
      }),
    );
  });
  result = {
    candidates,
    resolutions: requests.map((requested) => {
      const matching = candidates.filter((candidate) => candidate.requested === requested);
      const codes = matching
        .map((candidate) => candidate.rfiCode)
        .filter((code, index, all) => all.indexOf(code) === index);
      return {
        requested,
        rfiCodes: codes,
        missingFirstPage: Boolean(params.firstPage) && !matching.some(
          (candidate) => candidate.documentType === "firstPage",
        ),
        missingSketch: Boolean(params.sketch) && !matching.some(
          (candidate) => candidate.documentType === "sketch",
        ),
        ambiguous: codes.length > 1,
      };
    }),
    visited: candidates.length,
    truncated: false,
  } satisfies RfiPrintFindResult;
  return result;
}

export function diagnosticsOperation(
  params: Record<string, unknown>,
  onProgress?: ProgressHandler,
): unknown {
  let result: unknown;
  const content = `${JSON.stringify(
    {
      schemaVersion: 1,
      docflow: { version: __DOCFLOW_VERSION__ },
      platform: { arch: "demo", os: "browser-preview" },
      components: {
        ocr: {
          engine: "tesseract",
          languageDataAvailable: false,
          status: "missing",
        },
        sidecar: { required: false, status: "retired" },
        printer: {
          inventoryStatus: "available",
          printerCount: 1,
          defaultPrinterAvailable: true,
          latestQueue: null,
        },
      },
      recentErrorCodes: [],
      privacy: { delivery: "local-only", redaction: "recursive" },
    },
    null,
    2,
  )}\n`;
  result = {
    fileName: "docflow-support-diagnostics-v1.json",
    mediaType: "application/json",
    byteLength: new TextEncoder().encode(content).byteLength,
    content,
  } satisfies SupportBundlePreview;
  return result;
}


export function previewOperation(
  state: MockPlanState,
  params: Record<string, unknown>,
  onProgress?: ProgressHandler,
): unknown {
  let result: unknown;
  onProgress?.({ phase: "scan", done: 1, total: 1 });
  const values = params.values as Record<string, unknown>;
  // Mirror the Rust engine: a missing required value fails before any work.
  const missing = demoOperationList
    .find((operation) => operation.id === params.opId)
    ?.schema.find((param) => param.required && isEmptyValue(values[param.name]));
  if (missing) {
    throw new BridgeError("validation", `error.param.required.${missing.name}`);
  }
  if (params.opId === "rfi.complete_status_sync") {
    const code = "AZE-SUN-INT-QMS-RFI-MEC-0700";
    const workbook = mockPath(
      values.rfi_log_workbook,
      "/Users/demo/Documents/RFI Log.xlsm",
    );
    const row = {
      _rowId: "complete-row-1",
      enabled: true,
      selectable: true,
      discipline: "MEC",
      rfiNo: "MEC-0700",
      canonicalRfi: code,
      pdfPath: `MEC/${code}/RFI No. ${code}.pdf`,
      pdfCount: 1,
      pdfPaths: `MEC/${code}/RFI No. ${code}.pdf`,
      currentStatus: "Ongoing",
      proposedStatus: "Completed",
      action: "Set matching Inspection Status cell(s) to Completed",
      disposition: "rfi.completeStatus.disposition.ready",
      reasonKey: "Exact completed RFI PDF and safe RFI Log mapping found",
    };
    const plan = {
      planId: "demo-complete-status-plan",
      opId: "rfi.complete_status_sync",
      warnings: [],
      meta: {
        workerCount: 4,
        directoriesScanned: 42,
        entriesScanned: 500,
        exactPdfCount: 1,
        editor: {
          kind: "recordTable",
          layout: "compact",
          titleKey: "rfi.completeStatus.editor.title",
          rowCountKey: "rfi.completeStatus.editor.rowCount",
          groupField: "discipline",
          selectionGroupFields: [],
          selectionEnabled: true,
          bulkEnabled: true,
          detailsEnabled: true,
          detailsDefaultVisible: false,
          duplicateEnabled: false,
          requiredFields: ["enabled"],
          columns: [
            { key: "enabled", labelKey: "rfi.completeStatus.column.enabled", kind: "bool" },
            { key: "discipline", labelKey: "rfi.completeStatus.column.discipline", kind: "readonly" },
            { key: "rfiNo", labelKey: "rfi.completeStatus.column.rfiNo", kind: "readonly" },
            { key: "pdfPath", labelKey: "rfi.completeStatus.column.pdfPath", kind: "readonly" },
            { key: "currentStatus", labelKey: "rfi.completeStatus.column.currentStatus", kind: "readonly" },
            { key: "proposedStatus", labelKey: "rfi.completeStatus.column.proposedStatus", kind: "readonly" },
            { key: "disposition", labelKey: "rfi.completeStatus.column.disposition", kind: "readonly", display: "localizedKey" },
          ],
          rows: [row],
        },
      },
      steps: [
        {
          kind: "workbook_update" as const,
          src: workbook,
          dst: `${workbook}.completion-backup`,
          noteKey: "note.rfi.completeStatus.rfiLog",
          meta: {
            syncStep: {
              workbookKind: "rfiLogStatus",
              updates: {
                MEC: {
                  "10:16": {
                    rowId: row._rowId,
                    expectedValue: "Ongoing",
                    replacementValue: "Completed",
                    row: 10,
                    col: 16,
                  },
                },
              },
            },
          },
        },
      ],
    } satisfies Plan;
    state.completeStatusPlan = plan;
    result = plan;
    return result;
  }
  if (params.opId === "rfi.group_files") {
    const source = mockPath(values.source, "/Incoming");
    const destination = mockPath(values.destination, "/Grouped");
    const rows = [
      {
        _rowId: "file-group-0-0",
        enabled: true,
        rfiCode: "AZE-SUN-INT-QMS-RFI-CIV-1260",
        sourceFile: "AZE-SUN-INT-QMS-RFI-CIV-1260 SKETCH.pdf",
        method: "rfi.fileGroup.method.filename",
        confidence: 1,
      },
      {
        _rowId: "file-group-1-0",
        enabled: true,
        rfiCode: "AZE-SUN-INT-QMS-RFI-CIV-1261",
        sourceFile: "AZE-SUN-INT-QMS-RFI-CIV-1260_1261.eml",
        method: "rfi.fileGroup.method.mail",
        confidence: 1,
      },
    ];
    const plan = {
      planId: "demo-file-group-plan",
      opId: "rfi.group_files",
      warnings: [],
      meta: {
        sourceReadOnly: true,
        workerCount: 4,
        editor: {
          kind: "recordTable",
          layout: "compact",
          titleKey: "rfi.fileGroup.editor.title",
          rowCountKey: "rfi.fileGroup.editor.rowCount",
          groupField: "rfiCode",
          selectionGroupFields: ["rfiCode", "sourceFile"],
          selectionEnabled: true,
          bulkEnabled: true,
          requiredFields: ["enabled"],
          columns: [
            { key: "enabled", labelKey: "rfi.fileGroup.column.enabled", kind: "bool", width: 64 },
            { key: "rfiCode", labelKey: "rfi.fileGroup.column.rfiCode", kind: "readonly", width: 260, bulk: false },
            { key: "sourceFile", labelKey: "rfi.fileGroup.column.sourceFile", kind: "readonly", width: 300, bulk: false },
            { key: "method", labelKey: "rfi.fileGroup.column.method", kind: "readonly", display: "localizedKey", width: 120, bulk: false },
            { key: "confidence", labelKey: "ocr.confidence", kind: "readonly", display: "percent", width: 112, bulk: false },
          ],
          rows,
        },
      },
      steps: rows.flatMap((row) => [
        {
          kind: "mkdir" as const,
          src: null,
          dst: `${destination}/${row.rfiCode}`,
          noteKey: "note.rfi.fileGroup.folderCreated",
          meta: { code: row.rfiCode },
        },
        {
          kind: "copy" as const,
          src: `${source}/${row.sourceFile}`,
          dst: `${destination}/${row.rfiCode}/${row.sourceFile}`,
          noteKey: "note.rfi.fileGroup.copy",
          meta: { rowId: row._rowId, code: row.rfiCode },
        },
      ]),
    } satisfies Plan;
    state.fileGroupPlan = plan;
    result = plan;
    return result;
  }
  if (params.opId === "files.batch_print") {
    const files = Array.isArray(values.files)
      ? values.files.map((value) => mockPath(value, ""))
      : ["/Users/demo/Documents/first.pdf"];
    const rows = files.map((path, index) => ({
      _rowId: `file-${index + 1}`,
      enabled: true,
      status: "ready",
      file: path.split(/[\\/]/).pop() ?? path,
      path,
      format: path.split(".").pop()?.toUpperCase() ?? "FILE",
      pageRange: "all",
      printer: "Office Printer",
      paper: index ? "A3" : "A4",
      orientation: "auto",
      scaling: "fit",
      color: "printer",
      duplex: "printer",
      copies: 1,
    }));
    const plan = {
      planId: "demo-print-plan",
      opId: "files.batch_print",
      warnings: [],
      meta: {
        editor: {
          kind: "recordTable",
          titleKey: "print.editor.title",
          rowCountKey: "print.editor.rowCount",
          groupField: "format",
          selectionGroupFields: ["format"],
          selectionEnabled: true,
          bulkEnabled: true,
          columns: [
            { key: "enabled", labelKey: "print.col.include", kind: "bool", width: 64 },
            { key: "status", labelKey: "print.col.status", kind: "readonly", width: 90, bulk: false },
            { key: "file", labelKey: "print.col.file", kind: "readonly", width: 260, bulk: false },
            { key: "format", labelKey: "print.col.format", kind: "readonly", width: 76, bulk: false },
            { key: "pageRange", labelKey: "print.col.pageRange", helpKey: "print.col.pageRange.help", kind: "text", width: 112, bulk: false },
            { key: "printer", labelKey: "print.col.printer", kind: "choice", width: 180 },
            { key: "paper", labelKey: "print.col.paper", kind: "choice", width: 88 },
            { key: "orientation", labelKey: "print.col.orientation", kind: "choice", width: 110 },
            { key: "scaling", labelKey: "print.col.scaling", kind: "choice", width: 110 },
            { key: "color", labelKey: "print.col.color", kind: "choice", width: 100 },
            { key: "duplex", labelKey: "print.col.duplex", kind: "choice", width: 116 },
            { key: "copies", labelKey: "print.col.copies", kind: "int", width: 76 },
          ],
          choices: {
            printer: ["Office Printer"],
            paper: ["auto", "A4", "A3", "A5", "Letter", "Legal"],
            orientation: ["auto", "portrait", "landscape"],
            scaling: ["fit", "actual", "shrink"],
            color: ["printer", "color", "grayscale"],
            duplex: ["printer", "simplex", "long", "short"],
          },
          choiceLabels: {},
          rows,
        },
      },
      steps: rows.map((row) => ({
        kind: "print" as const,
        src: row.path,
        dst: row.path,
        noteKey: "note.print.queued",
        meta: {},
      })),
    } satisfies Plan;
    state.printPlan = plan;
    result = plan;
    return result;
  }
  if (params.opId === "rfi.normalize_pdf_orientation") {
    result = {
      planId: "demo-orientation-plan",
      opId: "rfi.normalize_pdf_orientation",
      warnings: [],
      meta: {
        sourceReadOnly: true,
        workerCount: 4,
        verifiedPages: 24,
        rotatedPages: 3,
        actionableFiles: 2,
        manualReviewFiles: 0,
        scanIndex: {
          discovered: 8,
          reused: 6,
          analyzed: 2,
          failed: 0,
          priorInterruptedRuns: 0,
          forceRescan: Boolean(values.force_rescan),
        },
      },
      steps: [
        {
          kind: "pdf_page_rotate",
          src: `${mockPath(values.rfi_system_root, "/RFI System")}/MEC/RFI-0001/document.pdf`,
          dst: `${mockPath(values.review_root, "/Orientation Review")}/MEC/RFI-0001/document.pdf`,
          noteKey: "note.rfi.pdfOrientationNormalized",
          meta: {
            rotations: [
              { pageIndex: 1, fromRotation: 0, targetRotation: 90 },
            ],
          },
        },
      ],
    } satisfies Plan;
    return result;
  }
  if (params.opId === "rfi.log_add") {
    const rows = [
      {
        _rowId: "CIV-g1-r1",
        _sheet: "CIV",
        rfiNo: "AZE-SUN-INT-QMS-RFI-CIV-0008",
        itpNo: "AZE-SUN-INT-QMS-ITP-CIV-0005",
        itemNo: "CIV-5.2.1",
        description: "Civil inspection description",
        forms: "CON-CIV-001",
        tagNo: "N12",
        area: "North Site",
        rowNo: "",
        dateOfIssue: "15-Aug-26",
        inspectionDate: "16-Aug-26",
        startTime: "09:00",
        azfenType: "H",
        intecType: "W",
        location: "N3",
        subcontractor: "AZFEN",
      },
      {
        _rowId: "CIV-g2-r1",
        _sheet: "CIV",
        rfiNo: "AZE-SUN-INT-QMS-RFI-CIV-0009",
        itpNo: "AZE-SUN-INT-QMS-ITP-CIV-0005",
        itemNo: "CIV-5.2.1",
        description: "Civil inspection description",
        forms: "CON-CIV-001",
        tagNo: "N18",
        area: "North Site",
        rowNo: "",
        dateOfIssue: "15-Aug-26",
        inspectionDate: "16-Aug-26",
        startTime: "09:30",
        azfenType: "H",
        intecType: "W",
        location: "N3",
        subcontractor: "PDL",
      },
    ];
    result = {
      planId: "demo-rfi-plan",
      opId: "rfi.log_add",
      warnings: [],
      meta: {
        editor: {
          kind: "recordTable",
          layout: "compact",
          titleKey: "rfi.editor.title",
          groupField: "rfiNo",
          selectionGroupFields: ["_sheet", "rfiNo"],
          selectionEnabled: true,
          bulkEnabled: true,
          detailsEnabled: true,
          detailsDefaultVisible: false,
          duplicateEnabled: true,
          columns: [
            { key: "_sheet", labelKey: "rfi.col.sheet", kind: "readonly", width: 42 },
            { key: "rfiNo", labelKey: "rfi.col.rfiNo", kind: "readonly", width: 54, display: "suffix" },
            { key: "itpNo", labelKey: "rfi.col.itpNo", kind: "choice", width: 50, display: "shortNumber" },
            { key: "itemNo", labelKey: "rfi.col.itemNo", kind: "choice", width: 62, display: "itemSuffix" },
            { key: "description", labelKey: "rfi.col.description", kind: "multiline", width: 360, detail: true },
            { key: "forms", labelKey: "rfi.col.forms", kind: "multiline", width: 280, detail: true },
            { key: "tagNo", labelKey: "rfi.col.tagNo", kind: "autocomplete", width: 92 },
            { key: "area", labelKey: "rfi.col.area", kind: "autocomplete", width: 94 },
            { key: "dateOfIssue", labelKey: "rfi.col.issueDate", kind: "compactDate", width: 116 },
            { key: "inspectionDate", labelKey: "rfi.col.inspectionDate", kind: "compactDate", width: 116 },
            { key: "startTime", labelKey: "rfi.col.startTime", kind: "text", width: 58 },
            { key: "azfenType", labelKey: "rfi.col.azfenType", kind: "text", width: 130, detail: true },
            { key: "intecType", labelKey: "rfi.col.intecType", kind: "text", width: 130, detail: true },
            { key: "location", labelKey: "rfi.col.location", kind: "autocomplete", width: 104 },
            { key: "subcontractor", labelKey: "rfi.col.subcontractor", kind: "autocomplete", width: 88 },
          ],
          choices: {
            itpNo: ["AZE-SUN-INT-QMS-ITP-CIV-0005"],
            itemNo: ["CIV-5.2.1"],
          },
          autocomplete: {
            tagNo: ["N12", "N18"],
            area: ["North Site", "South Site"],
            location: ["N3"],
            subcontractor: ["AZFEN", "PDL"],
          },
          rows,
        },
      },
      steps: [
        {
          kind: "workbook_update",
          src: mockPath(
            values.workbook,
            "/Users/demo/Documents/RFI Log.xlsx",
          ),
          dst: "/Users/demo/Documents/.docflow-backups/RFI Log.xlsx",
          noteKey: "note.rfi.workbookUpdate",
          meta: {},
        },
      ],
    } satisfies Plan;
    return result;
  }
  if (params.opId === demoFastCoverOperation.id) {
    const destination = mockPath(values.dest, "/RFI");
    const organize = values.organize_into_rfi_folders !== false;
    const code = "AZE-SUN-INT-QMS-RFI-CIV-0064";
    const discipline = code.split("-")[5];
    const target = `${destination}${
      organize ? `/${discipline}` : ""
    }/RFI No. ${code}.pdf`;
    const sourceFile = "combined_scan.pdf";
    const sourcePath = `${mockPath(values.source, "/Scans")}/${sourceFile}`;
    const row = {
      _rowId: "scan-output-0",
      sourceFile,
      evidenceKey: "rfi.fastScan.evidence.confirmed",
      detectedCode: code,
      code,
      target,
    };
    const plan = {
      planId: "demo-fast-scan-plan",
      opId: demoFastCoverOperation.id,
      warnings: [],
      meta: {
        scanEdit: {
          destinationRoot: destination,
          organizeIntoRfiFolders: organize,
        },
        editor: {
          kind: "rfiScan",
          titleKey: "rfi.fastScan.editor.title",
          rowCountKey: "rfi.fastScan.editor.rowCount",
          groupField: "_rowId",
          requiredFields: ["code"],
          columns: [
            {
              key: "sourceFile",
              labelKey: "rfi.fastScan.column.sourceFile",
              kind: "readonly",
              width: 170,
            },
            {
              key: "evidenceKey",
              labelKey: "rfi.fastScan.column.evidence",
              kind: "readonly",
              display: "localizedKey",
              width: 170,
            },
            {
              key: "detectedCode",
              labelKey: "rfi.fastScan.column.detectedCode",
              kind: "readonly",
              width: 270,
            },
            {
              key: "code",
              labelKey: "rfi.fastScan.column.code",
              kind: "text",
              width: 310,
            },
            {
              key: "target",
              labelKey: "rfi.fastScan.column.target",
              kind: "readonly",
              width: 360,
            },
          ],
          rows: [row],
        },
      },
      steps: [
        {
          kind: "copy",
          src: sourcePath,
          dst: target,
          noteKey: "note.renamed",
          meta: { code, scanEditRowId: row._rowId },
        },
      ],
    } satisfies Plan;
    state.fastScanPlan = plan;
    result = plan;
    return result;
  }
  result = {
    planId: "demo-plan",
    opId: String(params.opId),
    warnings: [],
    meta: {},
    steps: [
      {
        kind: "move",
        src: `${mockPath(values.source, "/Scans")}/20260718_163146.PDF`,
        dst: `${mockPath(values.dest, "/RFI")}/RFI-CIV-0064/RFI-CIV-0064-R0.pdf`,
        noteKey: "note.renamed",
        meta: { confidence: 1 },
      },
    ],
  } satisfies Plan;
  return result;
}

export function reviseOperation(
  state: MockPlanState,
  params: Record<string, unknown>,
  onProgress?: ProgressHandler,
): unknown {
  let result: unknown;
  if (state.fastScanPlan && params.planId === state.fastScanPlan.planId) {
    const editor = state.fastScanPlan.meta.editor as Record<
      string,
      unknown
    >;
    const originalRows = Array.isArray(editor.rows)
      ? (editor.rows as Record<string, unknown>[])
      : [];
    const submittedRows = Array.isArray(params.rows)
      ? (params.rows as Record<string, unknown>[])
      : [];
    const edit = state.fastScanPlan.meta.scanEdit as Record<
      string,
      unknown
    >;
    const destination = String(edit.destinationRoot ?? "");
    const organize = edit.organizeIntoRfiFolders === true;
    const seenCodes = new Set<string>();
    if (submittedRows.length !== originalRows.length) {
      throw new Error("error.rfi.fastScan.revisionInvalid");
    }
    const rows = submittedRows.map<Record<string, unknown>>(
      (submitted, index) => {
        const original = originalRows[index];
        const inputCode = submitted.code;
        const expected = { ...original, code: inputCode };
        if (JSON.stringify(submitted) !== JSON.stringify(expected)) {
          throw new Error("error.rfi.fastScan.revisionInvalid");
        }
        const code = normalizeMockRfiCode(inputCode);
        if (!code) {
          throw new Error("error.rfi.fastScan.codeInvalid");
        }
        if (seenCodes.has(code)) {
          throw new Error("error.rfi.fastScan.targetExists");
        }
        seenCodes.add(code);
        const discipline = code.split("-")[5];
        const target = `${destination}${
          organize ? `/${discipline}` : ""
        }/RFI No. ${code}.pdf`;
        return { ...expected, code, target };
      },
    );
    const steps = state.fastScanPlan.steps.map((step) => {
      const rowId = step.meta.scanEditRowId;
      const row = rows.find((candidate) => candidate._rowId === rowId);
      return row
        ? {
            ...step,
            dst: String(row.target),
            meta: { ...step.meta, code: row.code },
          }
        : step;
    });
    const revised = {
      ...state.fastScanPlan,
      planId: `${state.fastScanPlan.planId}-revised`,
      meta: {
        ...state.fastScanPlan.meta,
        editor: { ...editor, rows },
      },
      steps,
    } satisfies Plan;
    state.fastScanPlan = revised;
    result = revised;
    return result;
  }
  if (
    state.completeStatusPlan &&
    params.planId === state.completeStatusPlan.planId
  ) {
    const rows = Array.isArray(params.rows)
      ? params.rows.map((row) => ({ ...row }))
      : [];
    const editor = state.completeStatusPlan.meta.editor as Record<
      string,
      unknown
    >;
    const revised = {
      ...state.completeStatusPlan,
      planId: "demo-complete-status-plan-revised",
      meta: {
        ...state.completeStatusPlan.meta,
        editor: { ...editor, rows },
      },
      steps: rows.some((row) => row.enabled !== false)
        ? state.completeStatusPlan.steps
        : [],
    } satisfies Plan;
    state.completeStatusPlan = revised;
    result = revised;
    return result;
  }
  if (state.fileGroupPlan && params.planId === state.fileGroupPlan.planId) {
    const rows = Array.isArray(params.rows)
      ? params.rows.map((row) => ({ ...row }))
      : [];
    const editor = state.fileGroupPlan.meta.editor as Record<string, unknown>;
    const enabledRows = rows.filter((row) => row.enabled !== false);
    const revised = {
      ...state.fileGroupPlan,
      planId: "demo-file-group-plan-revised",
      meta: {
        ...state.fileGroupPlan.meta,
        editor: { ...editor, rows },
      },
      steps: state.fileGroupPlan.steps.filter((step) => {
        const rowId = String(step.meta.rowId ?? "");
        if (rowId) return enabledRows.some((row) => row._rowId === rowId);
        const code = String(step.meta.code ?? "");
        return !code || enabledRows.some((row) => row.rfiCode === code);
      }),
    } satisfies Plan;
    state.fileGroupPlan = revised;
    result = revised;
    return result;
  }
  if (!state.printPlan || params.planId !== state.printPlan.planId) {
    throw new Error("Unknown demo print plan");
  }
  const rows = Array.isArray(params.rows)
    ? params.rows.map((row) => ({ ...row }))
    : [];
  const editor = state.printPlan.meta.editor as Record<string, unknown>;
  const revised = {
    ...state.printPlan,
    planId: "demo-print-plan-revised",
    meta: {
      ...state.printPlan.meta,
      editor: { ...editor, rows },
    },
    steps: rows
      .filter((row) => row.enabled !== false)
      .map((row) => ({
        kind: "print" as const,
        src: String(row.path),
        dst: String(row.path),
        noteKey: "note.print.queued",
        meta: {},
      })),
  } satisfies Plan;
  state.printPlan = revised;
  result = revised;
  return result;
}

