import {
  BadgeCheck,
  CircleAlert,
  Database,
  Eye,
  FileSearch,
  Files,
  Shield,
} from "lucide-react";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";
import {
  StepRow,
} from "../screens/ResultView";
import {
  summarizePlanImpact,
} from "./planImpact";

export * from "./workbookCellChanges";
export * from "./planImpact";
import {
  scrollToSignedSection,
  signedWarningReviewItems,
} from "./signedWarnings";

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
