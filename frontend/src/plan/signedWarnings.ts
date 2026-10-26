import { type Plan } from "../types";

export interface SignedWarningReason {
  key: string;
  pageNumber: number | null;
  code: string;
  detail: string;
}

export interface SignedWarningReviewItem {
  sourceFile: string;
  destination: string;
  status: "planned" | "exists" | "blocked";
  reasons: SignedWarningReason[];
}

export function signedWarningReviewItems(plan: Plan): SignedWarningReviewItem[] {
  const raw = Array.isArray(plan.meta?.warningReviewItems)
    ? plan.meta.warningReviewItems
    : [];
  return raw.flatMap((value) => {
    if (!value || typeof value !== "object") return [];
    const item = value as Record<string, unknown>;
    if (
      typeof item.sourceFile !== "string" ||
      typeof item.destination !== "string" ||
      !["planned", "exists", "blocked"].includes(String(item.status)) ||
      !Array.isArray(item.reasons)
    ) {
      return [];
    }
    const reasons = item.reasons.flatMap((rawReason) => {
      if (!rawReason || typeof rawReason !== "object") return [];
      const reason = rawReason as Record<string, unknown>;
      if (typeof reason.key !== "string" || typeof reason.detail !== "string") {
        return [];
      }
      return [
        {
          key: reason.key,
          pageNumber:
            typeof reason.pageNumber === "number" &&
            Number.isSafeInteger(reason.pageNumber) &&
            reason.pageNumber > 0
              ? reason.pageNumber
              : null,
          code: typeof reason.code === "string" ? reason.code : "",
          detail: reason.detail,
        },
      ];
    });
    return [
      {
        sourceFile: item.sourceFile,
        destination: item.destination,
        status: item.status as SignedWarningReviewItem["status"],
        reasons,
      },
    ];
  });
}

export function scrollToSignedSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ block: "start" });
}
