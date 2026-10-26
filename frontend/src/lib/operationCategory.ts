export type OperationCategory = "all" | "scans" | "excel" | "mail" | "system";

const scanOperations = new Set([
  "rfi.process_scans_fast_covers",
  "rfi.normalize_pdf_orientation",
  "rfi.stage_signed_first_pages",
]);

const excelOperations = new Set([
  "rfi.log_add",
  "rfi.follow_up_sync",
  "rfi.complete_status_sync",
  "rfi.publish_status",
  "rfi.system_sync",
]);

export function getOperationCategory(id: string): OperationCategory {
  if (scanOperations.has(id)) return "scans";
  if (excelOperations.has(id)) return "excel";
  if (id === "rfi.group_files") return "mail";
  return "system";
}
