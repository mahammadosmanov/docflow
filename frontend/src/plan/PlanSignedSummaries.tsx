import {
  BadgeCheck,
  CircleAlert,
  FileSearch,
  Files,
} from "lucide-react";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";
import {
  scrollToSignedSection,
  signedWarningReviewItems,
} from "./signedWarnings";

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
