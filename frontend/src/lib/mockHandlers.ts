import { type ProgressHandler } from "./bridge";
import {
  type RfiPrintFindResult,
  type SupportBundlePreview,
} from "../types";

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


