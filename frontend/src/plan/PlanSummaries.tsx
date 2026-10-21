import {
  AdjustableDataRegion,
  ColumnResizeHandle,
} from "../components/AdjustableDataRegion";
import {
  BadgeCheck,
  CircleAlert,
  Database,
  Eye,
  FileSearch,
  Files,
  Shield,
  TableProperties,
} from "lucide-react";
import {
  useId,
  useState,
} from "react";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";
import {
  StepRow,
} from "../screens/ResultView";

export interface PlanImpact {
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

export function PlanImpactSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const impact = summarizePlanImpact(plan);
  const entries = (
    [
      ["plan.impact.outputs", impact.outputs],
      ["plan.impact.updates", impact.updates],
      ["plan.impact.moves", impact.moves],
      ["plan.impact.printJobs", impact.printJobs],
      ["plan.impact.drafts", impact.drafts],
      ["plan.impact.cleanup", impact.cleanup],
      ["plan.impact.warnings", impact.warnings],
    ] satisfies [string, number][]
  ).filter(([, count]) => count > 0);
  return (
    <section className="plan-impact" aria-labelledby="plan-impact-title">
      <div className="plan-impact-heading">
        <Shield aria-hidden="true" />
        <div>
          <strong id="plan-impact-title">{s.t("plan.impact.title")}</strong>
          <span>{s.t("plan.impact.previewOnly")}</span>
        </div>
      </div>
      {entries.length > 0 && (
        <dl className="plan-impact-counts">
          {entries.map(([key, count]) => (
            <div key={key}>
              <dt>{s.t(key)}</dt>
              <dd>{count}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

export function ScanIndexSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const raw = plan.meta?.scanIndex;
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const data = raw as Record<string, unknown>;
  const count = (key: string) =>
    typeof data[key] === "number" && Number.isFinite(data[key])
      ? Math.max(0, Math.trunc(data[key] as number))
      : 0;
  const entries: [string, number][] = [
    ["scanIndex.discovered", count("discovered")],
    ["scanIndex.reused", count("reused")],
    ["scanIndex.analyzed", count("analyzed")],
    ["scanIndex.failed", count("failed")],
    ["scanIndex.interrupted", count("priorInterruptedRuns")],
  ];
  const visibleEntries = entries.filter(
    ([key, value]) => key !== "scanIndex.interrupted" || value > 0,
  );
  return (
    <section
      className="plan-impact scan-index-summary"
      aria-labelledby="scan-index-title"
    >
      <div className="plan-impact-heading">
        <Database aria-hidden="true" />
        <div>
          <strong id="scan-index-title">{s.t("scanIndex.title")}</strong>
          <span>
            {data.forceRescan === true
              ? s.t("scanIndex.forceApplied")
              : s.t("scanIndex.help")}
          </span>
        </div>
      </div>
      <dl className="plan-impact-counts">
        {visibleEntries.map(([key, value]) => (
          <div key={key}>
            <dt>{s.t(key)}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

export function PlanExactEffects({ plan, s }: { plan: Plan; s: Strings }) {
  const effects = plan.steps.filter((step) => step.kind !== "mkdir");
  if (effects.length === 0) return null;
  return (
    <details className="plan-exact-effects">
      <summary>
        <Eye aria-hidden="true" />
        <span>{s.t("plan.exact.title", { count: effects.length })}</span>
        <small>{s.t("plan.exact.help")}</small>
      </summary>
      <div className="plan-exact-list">
        {effects.map((step, index) => (
          <StepRow key={`${step.kind}-${index}`} step={step} s={s} fullPath />
        ))}
      </div>
    </details>
  );
}

export function canonicalRfiInText(value: string): string | null {
  return (
    value.match(
      /(?:^|[^A-Z0-9-])(AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4})(?=$|[^A-Z0-9-])/,
    )?.[1] ?? null
  );
}

export type WorkbookCellChange = {
  workbook: string;
  sheet: string;
  cell: string;
  before: string;
  after: string;
  rfiCode: string;
};

export const MAX_WORKBOOK_CELL_PREVIEWS = 1_000;

export function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function previewText(value: unknown): string | null {
  return typeof value === "string" && value.length <= 500 ? value : null;
}

export function excelCell(row: unknown, column: unknown): string | null {
  if (
    typeof row !== "number" ||
    typeof column !== "number" ||
    !Number.isInteger(row) ||
    !Number.isInteger(column) ||
    row < 1 ||
    row > 1_048_576 ||
    column < 1 ||
    column > 16_384
  ) {
    return null;
  }
  let value = column;
  let letters = "";
  while (value > 0) {
    value -= 1;
    letters = String.fromCharCode(65 + (value % 26)) + letters;
    value = Math.floor(value / 26);
  }
  return `${letters}${row}`;
}

export function summarizeWorkbookCellChanges(plan: Plan): WorkbookCellChange[] {
  const result: WorkbookCellChange[] = [];
  const rowCodes = new Map<string, string>();
  const editor = recordValue(plan.meta?.editor);
  const editorRows = Array.isArray(editor?.rows) ? editor.rows : [];
  for (const rawRow of editorRows.slice(0, 2_000)) {
    const row = recordValue(rawRow);
    const rowId = previewText(row?._rowId);
    const code = previewText(row?.canonicalRfi) ?? previewText(row?.rfiNo);
    if (rowId && code && canonicalRfiInText(code) === code)
      rowCodes.set(rowId, code);
  }
  const add = (change: WorkbookCellChange) => {
    if (result.length < MAX_WORKBOOK_CELL_PREVIEWS) result.push(change);
  };
  for (const step of plan.steps) {
    if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    if (step.kind !== "workbook_update") continue;
    const sync = recordValue(step.meta?.syncStep);
    const syncUpdates = recordValue(sync?.updates);
    if (sync && syncUpdates) {
      const workbook =
        sync.workbookKind === "rfiLog" || sync.workbookKind === "followUp"
          ? sync.workbookKind
          : "workbook";
      for (const [sheetName, rawSheet] of Object.entries(syncUpdates)) {
        const sheet = previewText(sheetName);
        const sheetUpdates = recordValue(rawSheet);
        if (!sheet || !sheetUpdates) continue;
        for (const rawUpdate of Object.values(sheetUpdates)) {
          const update = recordValue(rawUpdate);
          const cell = excelCell(update?.row, update?.col);
          const before = previewText(update?.expectedValue);
          const after = previewText(update?.replacementValue);
          const rowId = previewText(update?.rowId);
          if (!cell || before === null || after === null || !rowId) continue;
          add({
            workbook,
            sheet,
            cell,
            before,
            after,
            rfiCode: rowCodes.get(rowId) ?? "",
          });
          if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
        }
      }
      continue;
    }
    const updates = Array.isArray(step.meta?.updates) ? step.meta.updates : [];
    for (const rawUpdate of updates) {
      const update = recordValue(rawUpdate);
      const target = recordValue(update?.target);
      const worksheet = recordValue(target?.worksheet);
      const statusCell = recordValue(target?.statusCell);
      const rfiCell = recordValue(target?.rfiCell);
      const sheet = previewText(worksheet?.name);
      const cell = excelCell(target?.row, target?.statusColumn);
      const before = previewText(statusCell?.value);
      const after = previewText(update?.proposedStatus);
      const candidateCode = previewText(update?.canonicalRfi) ?? "";
      const rfiCode =
        canonicalRfiInText(candidateCode) === candidateCode
          ? candidateCode
          : "";
      const rfiAddress = excelCell(target?.row, target?.rfiColumn);
      const rfiBefore = previewText(rfiCell?.value);
      const rfiAfter = previewText(update?.proposedRfi);
      if (
        sheet &&
        rfiAddress &&
        rfiBefore !== null &&
        rfiAfter !== null &&
        rfiBefore !== rfiAfter
      ) {
        add({
          workbook: "followUp",
          sheet,
          cell: rfiAddress,
          before: rfiBefore,
          after: rfiAfter,
          rfiCode,
        });
        if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
      }
      if (!sheet || !cell || before === null || after === null) continue;
      add({ workbook: "followUp", sheet, cell, before, after, rfiCode });
      if (result.length >= MAX_WORKBOOK_CELL_PREVIEWS) return result;
    }
  }
  return result;
}

export function WorkbookChangePreview({ plan, s }: { plan: Plan; s: Strings }) {
  const changes = summarizeWorkbookCellChanges(plan);
  const headerId = useId();
  const [widths, setWidths] = useState([170, 150, 220, 260, 260]);
  const columns = [
    "plan.workbookChanges.workbook",
    "plan.workbookChanges.sheetCell",
    "plan.workbookChanges.rfi",
    "plan.workbookChanges.before",
    "plan.workbookChanges.after",
  ];
  const labels = columns.map((key) => s.t(key));
  const tableWidth = widths.reduce((total, width) => total + width, 0);
  if (changes.length === 0) return null;
  return (
    <details className="workbook-change-preview">
      <summary>
        <span>
          <TableProperties aria-hidden="true" />
          <strong>{s.t("plan.workbookChanges.title")}</strong>
        </span>
        <small>
          {s.t("plan.workbookChanges.count", { count: changes.length })}
        </small>
      </summary>
      <p>{s.t("plan.workbookChanges.help")}</p>
      <AdjustableDataRegion
        s={s}
        className="workbook-change-table-wrap"
        label={s.t("plan.workbookChanges.title")}
      >
        <table
          style={{ tableLayout: "fixed", width: `${tableWidth}px`, minWidth: 0 }}
        >
          <colgroup>
            {widths.map((width, index) => (
              <col key={columns[index]} style={{ width }} />
            ))}
          </colgroup>
          <thead>
            <tr>
              {labels.map((label, index) => (
                <th
                  key={columns[index]}
                  className="adjustable-column-header"
                  scope="col"
                  aria-labelledby={`${headerId}-${index}`}
                  style={{ width: widths[index] }}
                  title={label}
                >
                  <span id={`${headerId}-${index}`}>{label}</span>
                  <ColumnResizeHandle
                    width={widths[index]}
                    label={label}
                    s={s}
                    onWidthChange={(width) =>
                      setWidths((current) =>
                        current.map((existing, columnIndex) =>
                          columnIndex === index ? width : existing,
                        ),
                      )
                    }
                  />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {changes.map((change, index) => (
              <tr
                key={`${change.workbook}-${change.sheet}-${change.cell}-${index}`}
              >
                <td>{s.t(`plan.workbookChanges.kind.${change.workbook}`)}</td>
                <td>
                  <code>
                    {change.sheet}!{change.cell}
                  </code>
                </td>
                <td>{change.rfiCode || "—"}</td>
                <td>{change.before || s.t("plan.workbookChanges.blank")}</td>
                <td>{change.after || s.t("plan.workbookChanges.blank")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </AdjustableDataRegion>
      {changes.length === MAX_WORKBOOK_CELL_PREVIEWS && (
        <small className="workbook-change-limit">
          {s.t("plan.workbookChanges.limit", {
            count: MAX_WORKBOOK_CELL_PREVIEWS,
          })}
        </small>
      )}
    </details>
  );
}

export function PlanWarnings({
  warnings,
  focusableCodes,
  onFocusCode,
  s,
}: {
  warnings: Plan["warnings"];
  focusableCodes: string[];
  onFocusCode?: (code: string) => void;
  s: Strings;
}) {
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const types = [...new Set(warnings.map((warning) => warning.key))];
  const normalizedQuery = query.trim().toLocaleLowerCase(s.locale);
  const visible = warnings.filter((warning) => {
    if (type !== "all" && warning.key !== type) return false;
    if (!normalizedQuery) return true;
    return `${warning.subject} ${s.t(warning.key)}`
      .toLocaleLowerCase(s.locale)
      .includes(normalizedQuery);
  });
  const focusable = new Set(focusableCodes);
  return (
    <section
      id="signed-warning-list"
      className="warning-review"
      aria-labelledby="warning-review-title"
    >
      <div className="warning-review-heading">
        <div>
          <CircleAlert aria-hidden="true" />
          <strong id="warning-review-title">{s.t("ui.warnings")}</strong>
          <span>{warnings.length}</span>
        </div>
        {warnings.length >= 5 && (
          <div className="warning-review-filters">
            <input
              type="search"
              aria-label={s.t("plan.warnings.search")}
              placeholder={s.t("plan.warnings.search")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <select
              aria-label={s.t("plan.warnings.type")}
              value={type}
              onChange={(event) => setType(event.target.value)}
            >
              <option value="all">{s.t("plan.warnings.all")}</option>
              {types.map((key) => (
                <option key={key} value={key}>
                  {s.t(key)}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>
      <div className="warning-review-list" role="list">
        {visible.map((warning, index) => {
          const code = canonicalRfiInText(warning.subject);
          const canFocus = Boolean(code && focusable.has(code) && onFocusCode);
          return (
            <div
              key={`${warning.key}-${warning.subject}-${index}`}
              role="listitem"
            >
              <div>
                {warning.subject && <strong>{warning.subject}</strong>}
                <span>{s.t(warning.key)}</span>
              </div>
              {canFocus && code && (
                <button type="button" onClick={() => onFocusCode?.(code)}>
                  <Eye aria-hidden="true" />
                  {s.t("plan.warnings.focus")}
                </button>
              )}
            </div>
          );
        })}
        {visible.length === 0 && <p>{s.t("plan.warnings.none")}</p>}
      </div>
      {warnings.length >= 5 && (
        <small>
          {s.t("plan.warnings.showing", {
            shown: visible.length,
            count: warnings.length,
          })}
        </small>
      )}
    </section>
  );
}

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

export function SignedStageNavigator({
  plan,
  rows,
  s,
}: {
  plan: Plan;
  rows: Record<string, unknown>[];
  s: Strings;
}) {
  const reviewItems = signedWarningReviewItems(plan);
  const approved = rows.filter((row) => row.enabled !== false).length;
  return (
    <nav className="signed-stage-nav" aria-label={s.t("rfi.signed.nav.label")}>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-approved-records")}
      >
        <BadgeCheck aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.approved")}</span>
        <strong>{approved}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-page-comparison")}
      >
        <Files aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.compare")}</span>
        <strong>{rows.length}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-warning-review")}
        disabled={reviewItems.length === 0}
      >
        <CircleAlert aria-hidden="true" />
        <span>{s.t("rfi.signed.nav.review")}</span>
        <strong>{reviewItems.length}</strong>
      </button>
      <button
        type="button"
        onClick={() => scrollToSignedSection("signed-warning-list")}
        disabled={plan.warnings.length === 0}
      >
        <FileSearch aria-hidden="true" />
        <span>{s.t("ui.warnings")}</span>
        <strong>{plan.warnings.length}</strong>
      </button>
    </nav>
  );
}

export function SignedWarningReviewSummary({ plan, s }: { plan: Plan; s: Strings }) {
  const items = signedWarningReviewItems(plan);
  if (items.length === 0) return null;
  return (
    <section
      id="signed-warning-review"
      className="signed-warning-review"
      aria-labelledby="signed-warning-review-title"
    >
      <header>
        <div>
          <CircleAlert aria-hidden="true" />
          <span>
            <strong id="signed-warning-review-title">
              {s.t("rfi.signed.warningReview.title")}
            </strong>
            <small>{s.t("rfi.signed.warningReview.help")}</small>
          </span>
        </div>
        <b>{items.length}</b>
      </header>
      <div className="signed-warning-review-list">
        {items.map((item) => (
          <article key={`${item.sourceFile}-${item.destination}`}>
            <div className="signed-warning-review-file">
              <strong>{item.sourceFile}</strong>
              <span className={`review-copy-status ${item.status}`}>
                {s.t(`rfi.signed.warningReview.${item.status}`)}
              </span>
            </div>
            <small>
              {s.t("rfi.signed.warningReview.destination")}: {item.destination}
            </small>
            <ul>
              {item.reasons.map((reason, index) => (
                <li key={`${reason.key}-${reason.pageNumber}-${index}`}>
                  <span>
                    {reason.pageNumber
                      ? s.t("rfi.signed.warningReview.page", {
                          page: reason.pageNumber,
                        })
                      : s.t("rfi.signed.warningReview.file")}
                    {reason.code ? ` · ${reason.code}` : ""}
                  </span>
                  <strong>{s.t(reason.key)}</strong>
                  {reason.detail && <small>{reason.detail}</small>}
                </li>
              ))}
            </ul>
          </article>
        ))}
      </div>
    </section>
  );
}
