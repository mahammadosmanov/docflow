import { type Strings } from "../i18n";
import { type PlanRowFilter } from "./editorRules";

/** Search box and filter tabs above the plan table, with the filtered row count. */
export function PlanFilterBar({
  searchTerm,
  filterMode,
  onSearchTermChange,
  onFilterModeChange,
  filteredCount,
  totalCount,
  s,
}: {
  searchTerm: string;
  filterMode: PlanRowFilter;
  onSearchTermChange: (value: string) => void;
  onFilterModeChange: (mode: PlanRowFilter) => void;
  filteredCount: number;
  totalCount: number;
  s: Strings;
}) {
  return (
    <div className="table-filter-bar">
      <div className="table-search-box">
        <input
          type="search"
          aria-label={s.t("filter.searchPlaceholder")}
          placeholder={s.t("filter.searchPlaceholder")}
          value={searchTerm}
          onChange={(event) => onSearchTermChange(event.target.value)}
        />
      </div>
      <div className="table-filter-tabs">
        <button
          type="button"
          className={`filter-tab ${filterMode === "all" ? "filter-tab--active" : ""}`}
          aria-pressed={filterMode === "all"}
          onClick={() => onFilterModeChange("all")}
        >
          {s.t("filter.all")}
        </button>
        <button
          type="button"
          className={`filter-tab ${filterMode === "changes" ? "filter-tab--active" : ""}`}
          aria-pressed={filterMode === "changes"}
          onClick={() => onFilterModeChange("changes")}
        >
          {s.t("filter.changes")}
        </button>
        <button
          type="button"
          className={`filter-tab ${filterMode === "review" ? "filter-tab--active" : ""}`}
          aria-pressed={filterMode === "review"}
          onClick={() => onFilterModeChange("review")}
        >
          {s.t("filter.manualReview")}
        </button>
      </div>
      {(searchTerm !== "" || filterMode !== "all") && (
        <span className="table-filter-count">
          {s.t("table.filterCount", {
            filtered: filteredCount,
            total: totalCount,
          })}
        </span>
      )}
    </div>
  );
}
