import {
  AdjustableDataRegion,
  ColumnResizeHandle,
} from "../components/AdjustableDataRegion";
import {
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
  MAX_WORKBOOK_CELL_PREVIEWS,
  summarizeWorkbookCellChanges,
} from "./workbookCellChanges";

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
