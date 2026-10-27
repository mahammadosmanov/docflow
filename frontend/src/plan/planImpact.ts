import { type Plan } from "../types";

interface PlanImpact {
  outputs: number;
  updates: number;
  moves: number;
  printJobs: number;
  drafts: number;
  cleanup: number;
  warnings: number;
}

export function summarizePlanImpact(plan: Plan): PlanImpact {
  const impact: PlanImpact = {
    outputs: 0,
    updates: 0,
    moves: 0,
    printJobs: 0,
    drafts: 0,
    cleanup: 0,
    warnings: plan.warnings.length,
  };
  for (const step of plan.steps) {
    switch (step.kind) {
      case "copy":
      case "write":
      case "pdf_first_page_replace":
      case "pdf_page_rotate":
      case "pdf_pages_extract":
        impact.outputs += 1;
        break;
      case "workbook_update":
      case "restore_backup":
      case "replica_publish":
        impact.updates += 1;
        break;
      case "move":
      case "rename":
        impact.moves += 1;
        break;
      case "print":
        impact.printJobs += 1;
        break;
      case "outlook_reply_draft":
        impact.drafts += 1;
        break;
      case "cleanup_temp":
        impact.cleanup += 1;
        break;
      case "mkdir":
        break;
    }
  }
  return impact;
}
