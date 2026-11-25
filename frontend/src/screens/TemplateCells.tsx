import { Tag, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/ui";
import { Notice } from "../components/feedback";
import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import {
  type CellAnchor,
  type CellProblem,
  type TemplateField,
  type TemplateStructure,
} from "../types";
import { fieldKey } from "./TemplateDocument";

const MAX_START_CHARS = 119;

/** `3` becomes `C`, `27` becomes `AA`. */
function columnLetters(column: number): string {
  let letters = "";
  for (let rest = column; rest > 0; rest = Math.floor((rest - 1) / 26)) {
    letters = String.fromCharCode(65 + ((rest - 1) % 26)) + letters;
  }
  return letters;
}

/** Plain-language place of a marked cell, for the field list and the toolbar. */
export function cellLocation(s: Strings, anchor: CellAnchor): string {
  if (anchor.sheet !== undefined) {
    return s.t("templates.cells.where.excel", {
      address: anchor.address ?? "",
      sheet: anchor.sheet,
    });
  }
  return s.t("templates.cells.where.word", {
    table: (anchor.table ?? 0) + 1,
    row: (anchor.row ?? 0) + 1,
    cell: (anchor.cell ?? 0) + 1,
  });
}

const sameAnchor = (a: CellAnchor, b: CellAnchor) =>
  a.sheet === b.sheet &&
  a.address === b.address &&
  a.table === b.table &&
  a.row === b.row &&
  a.cell === b.cell;

interface Picked {
  anchor: CellAnchor;
  text: string;
}

/** Shows the template's tables or sheets; a selected cell becomes an editable part. */
export function TemplateCells({
  bridge,
  s,
  versionId,
  fields,
  reloadKey,
  onMark,
}: {
  bridge: Bridge;
  s: Strings;
  versionId: string;
  fields: TemplateField[];
  /** Changes when the draft's file changes, so the view reloads. */
  reloadKey: string;
  onMark: (field: TemplateField) => void;
}) {
  const [structure, setStructure] = useState<TemplateStructure | null | undefined>(undefined);
  const [sheetIndex, setSheetIndex] = useState(0);
  const [picked, setPicked] = useState<Picked | null>(null);
  const [label, setLabel] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let current = true;
    bridge
      .call("templates/cells", { versionId })
      .then((result) => {
        if (current) setStructure(result?.kind ? result : null);
      })
      .catch((reason) => {
        if (current) {
          setStructure(null);
          setError(humanError(reason, s));
        }
      });
    return () => {
      current = false;
    };
  }, [bridge, versionId, reloadKey, s]);

  const marked = useMemo(
    () => fields.flatMap((field) => (field.anchor ? [field.anchor] : [])),
    [fields],
  );
  const isMarked = (anchor: CellAnchor) => marked.some((item) => sameAnchor(item, anchor));

  const choose = (anchor: CellAnchor, text: string) => {
    setPicked({ anchor, text });
    setLabel(text.trim().slice(0, 40));
  };

  const mark = (field: Omit<TemplateField, "options" | "multiline">) => {
    onMark({ options: [], multiline: false, ...field });
    setPicked(null);
  };

  const makeEditable = () => {
    if (!picked) return;
    const name = label.trim() || cellLocation(s, picked.anchor);
    mark({
      key: fieldKey(name, fields.map((field) => field.key)),
      label: name,
      value: picked.text.length <= MAX_START_CHARS ? picked.text : "",
      anchor: picked.anchor,
    });
  };

  const problemTitle = (problem: CellProblem | "formula" | "protected") =>
    s.t(`templates.cells.locked.${problem}`);

  const cellClass = (anchor: CellAnchor, locked: boolean) =>
    [
      "cell-grid-cell",
      isMarked(anchor) ? "marked" : "",
      picked && sameAnchor(picked.anchor, anchor) ? "picked" : "",
      locked ? "locked" : "",
    ]
      .filter(Boolean)
      .join(" ");

  const suggestions = (structure?.suggestions ?? []).filter(
    (item) =>
      !fields.some((field) => field.key === item.key) &&
      !isMarked(item.anchor),
  );

  const renderExcel = () => {
    const sheets = structure?.sheets ?? [];
    const sheet = sheets[Math.min(sheetIndex, sheets.length - 1)];
    if (!sheet) return <p className="muted">{s.t("templates.cells.empty")}</p>;
    const byPlace = new Map(sheet.cells.map((cell) => [`${cell.row}:${cell.column}`, cell]));
    const spans = new Map(sheet.merges.map((m) => [`${m.row}:${m.column}`, m]));
    const covered = new Set<string>();
    for (const merge of sheet.merges) {
      for (let row = merge.row; row < merge.row + merge.rowSpan; row += 1) {
        for (let column = merge.column; column < merge.column + merge.colSpan; column += 1) {
          if (row !== merge.row || column !== merge.column) covered.add(`${row}:${column}`);
        }
      }
    }
    const rows = Array.from({ length: sheet.maxRow }, (_, index) => index + 1);
    const columns = Array.from({ length: sheet.maxColumn }, (_, index) => index + 1);
    return (
      <>
        {sheets.length > 1 && (
          <div className="category-filter-chips" role="group" aria-label={s.t("templates.cells.sheets")}>
            {sheets.map((item, index) => (
              <button
                key={item.name}
                type="button"
                className={`category-chip ${index === sheetIndex ? "active" : ""}`}
                aria-pressed={index === sheetIndex}
                onClick={() => setSheetIndex(index)}
              >
                {item.name}
              </button>
            ))}
          </div>
        )}
        {sheet.truncated && <p className="muted">{s.t("templates.cells.truncated")}</p>}
        <div className="cell-grid-scroll">
          <table className="cell-grid">
            <tbody>
              {rows.map((row) => (
                <tr key={row}>
                  <th scope="row">{row}</th>
                  {columns.map((column) => {
                    const place = `${row}:${column}`;
                    if (covered.has(place)) return null;
                    const cell = byPlace.get(place);
                    const merge = spans.get(place);
                    const anchor = { sheet: sheet.name, address: `${columnLetters(column)}${row}` };
                    const problem = cell?.formula ? "formula" : sheet.protected ? "protected" : null;
                    const text = cell?.text ?? "";
                    return (
                      <td
                        key={column}
                        rowSpan={merge?.rowSpan}
                        colSpan={merge?.colSpan}
                        className={cellClass(anchor, problem !== null)}
                        title={problem ? problemTitle(problem) : anchor.address}
                        aria-disabled={problem !== null || undefined}
                        role="button"
                        aria-label={text || anchor.address}
                        tabIndex={problem ? -1 : 0}
                        onClick={() => !problem && choose(anchor, text)}
                        onKeyDown={(event) => {
                          if (!problem && (event.key === "Enter" || event.key === " ")) {
                            event.preventDefault();
                            choose(anchor, text);
                          }
                        }}
                      >
                        {text}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </>
    );
  };

  const renderWord = () => {
    const tables = structure?.tables ?? [];
    if (tables.length === 0) return <p className="muted">{s.t("templates.cells.empty")}</p>;
    return tables.map((table, tableIndex) => (
      <div key={tableIndex} className="cell-grid-scroll">
        {tables.length > 1 && <h3>{s.t("templates.cells.table", { n: tableIndex + 1 })}</h3>}
        {table.truncated && <p className="muted">{s.t("templates.cells.truncated")}</p>}
        <table className="cell-grid">
          <tbody>
            {table.rows.map((row, rowIndex) => (
              <tr key={rowIndex}>
                {row.cells.map((cell, cellIndex) => {
                  const anchor = { table: tableIndex, row: rowIndex, cell: cellIndex };
                  return (
                    <td
                      key={cellIndex}
                      colSpan={cell.colSpan}
                      className={cellClass(anchor, cell.problem !== null)}
                      title={cell.problem ? problemTitle(cell.problem) : undefined}
                      aria-disabled={cell.problem !== null || undefined}
                      role="button"
                      aria-label={cell.text || cellLocation(s, anchor)}
                      tabIndex={cell.problem ? -1 : 0}
                      onClick={() => !cell.problem && choose(anchor, cell.text)}
                      onKeyDown={(event) => {
                        if (!cell.problem && (event.key === "Enter" || event.key === " ")) {
                          event.preventDefault();
                          choose(anchor, cell.text);
                        }
                      }}
                    >
                      {cell.text}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    ));
  };

  return (
    <section className="content-card template-editor-section">
      <h2>{s.t("templates.cells.title")}</h2>
      <p className="muted">{s.t("templates.cells.help")}</p>
      {suggestions.length > 0 && (
        <div className="template-editor-actions">
          {suggestions.map((item) => (
            <Button
              key={`${item.key}:${cellLocation(s, item.anchor)}`}
              variant="secondary"
              icon={<Tag />}
              onClick={() =>
                mark({ key: item.key, label: item.label, value: item.value, anchor: item.anchor })
              }
            >
              {s.t("templates.cells.suggested")} {s.t("templates.cells.suggest.rfi")}
            </Button>
          ))}
        </div>
      )}
      {picked && (
        <div className="template-selection" role="status">
          <span>
            {s.t("templates.cells.selected")} <strong>{cellLocation(s, picked.anchor)}</strong>
          </span>
          <label className="field cell-label">
            <span>{s.t("templates.editor.label")}</span>
            <input value={label} onChange={(event) => setLabel(event.target.value)} />
          </label>
          <Button icon={<Tag />} onClick={makeEditable}>
            {s.t("templates.cells.makeEditable")}
          </Button>
          <Button variant="ghost" icon={<X />} onClick={() => setPicked(null)}>
            {s.t("action.cancel")}
          </Button>
        </div>
      )}
      {error && <Notice kind="error">{error}</Notice>}
      {structure === undefined && <p className="muted">{s.t("templates.cells.loading")}</p>}
      {structure && (structure.kind === "xlsx" ? renderExcel() : renderWord())}
    </section>
  );
}
