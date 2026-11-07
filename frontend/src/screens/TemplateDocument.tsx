import { Pencil, Tag, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button } from "../components/ui";
import { Modal } from "../components/Modal";
import { Notice } from "../components/feedback";
import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { type TemplateDraft, type TemplateField } from "../types";

interface TextBlock {
  part: string;
  index: number;
  text: string;
}

interface Selection {
  part: string;
  index: number;
  start: number;
  end: number;
  text: string;
}

/** RFI Log columns a field can come from, named as in the RFI Log. */
export const LOG_SOURCES: [string, string][] = [
  ["rfiNo", "RFI No."],
  ["itpNo", "ITP No."],
  ["itemNo", "Item No."],
  ["description", "Inspection description"],
  ["forms", "Forms"],
  ["tagNo", "Tag No."],
  ["area", "Area"],
  ["location", "Insp. location / area"],
  ["dateOfIssue", "Date of issue"],
  ["inspectionDate", "Inspection date"],
  ["startTime", "Inspection start time"],
  ["azfenType", "Insp. type AZFEN"],
  ["intecType", "Insp. type INTEC"],
  ["subcontractor", "Subcontractor"],
  ["today", "Today"],
];

const PLACEHOLDER = /(\{\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}\})/g;

/** A field key from a label: ASCII camelCase, unique among `taken`. */
export function fieldKey(label: string, taken: string[]): string {
  const words = label
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/ı/g, "i")
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean);
  let base = words
    .map((word, index) =>
      index === 0
        ? word.toLowerCase()
        : word[0].toUpperCase() + word.slice(1).toLowerCase(),
    )
    .join("")
    .slice(0, 48);
  if (!/^[A-Za-z]/.test(base)) base = `field${base}`;
  return uniqueKey(base, taken);
}

function uniqueKey(base: string, taken: string[]): string {
  let key = base;
  for (let n = 2; taken.includes(key); n += 1) key = `${base}${n}`;
  return key;
}

/** Shows the template's text; selected text becomes a field or changes. */
export function TemplateDocument({
  bridge,
  s,
  draft,
  fields,
  beforeEdit,
  onEdited,
}: {
  bridge: Bridge;
  s: Strings;
  draft: TemplateDraft;
  fields: TemplateField[];
  beforeEdit: () => Promise<void>;
  onEdited: (next: TemplateDraft) => void;
}) {
  const [blocks, setBlocks] = useState<TextBlock[] | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [mode, setMode] = useState<"field" | "text" | null>(null);
  const [label, setLabel] = useState("");
  const [source, setSource] = useState("manual");
  const [replacement, setReplacement] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const result = await bridge.call("templates/blocks", { versionId: draft.versionId });
      setBlocks(Array.isArray(result?.blocks) ? result.blocks : []);
    } catch (reason) {
      setError(humanError(reason, s));
    }
  }, [bridge, draft.versionId, s]);

  useEffect(() => {
    void load();
    // A replaced or edited file has a new checksum; reload its text.
  }, [load, draft.placeholders.join("|")]);

  const capture = (block: TextBlock, element: HTMLElement) => {
    const selected = window.getSelection();
    if (!selected || selected.rangeCount === 0 || selected.isCollapsed) return;
    const range = selected.getRangeAt(0);
    if (!element.contains(range.startContainer) || !element.contains(range.endContainer)) {
      return;
    }
    const before = document.createRange();
    before.selectNodeContents(element);
    before.setEnd(range.startContainer, range.startOffset);
    const start = before.toString().length;
    const text = range.toString();
    if (!text.trim() || text.includes("{{") || text.includes("}}")) return;
    setSelection({ part: block.part, index: block.index, start, end: start + text.length, text });
    setError("");
  };

  const submit = async () => {
    if (!selection) return;
    setBusy(true);
    setError("");
    try {
      await beforeEdit();
      const existing = fields.find((field) => `existing:${field.key}` === source);
      const name =
        label.trim() ||
        LOG_SOURCES.find(([token]) => token === source)?.[1] ||
        selection.text.trim();
      const field =
        mode === "field"
          ? existing
            ? { key: existing.key, label: existing.label, value: existing.value }
            : {
                key: uniqueKey(
                  source === "manual" ? fieldKey(name, []) : source,
                  fields.map((item) => item.key),
                ),
                label: name,
                value: source === "manual" ? selection.text.trim() : `{${source}}`,
              }
          : undefined;
      const next = await bridge.call("templates/editText", {
        versionId: draft.versionId,
        part: selection.part,
        index: selection.index,
        start: selection.start,
        end: selection.end,
        expected: selection.text,
        replacement: mode === "text" ? replacement : "",
        ...(field ? { field } : {}),
      });
      setMode(null);
      setSelection(null);
      onEdited(next);
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  const open = (next: "field" | "text") => {
    if (!selection) return;
    setMode(next);
    setLabel("");
    setSource("manual");
    setReplacement(selection.text);
  };

  return (
    <section className="content-card template-editor-section">
      <h2>{s.t("templates.document.title")}</h2>
      <p className="muted">{s.t("templates.document.help")}</p>
      {selection && (
        <div className="template-selection" role="status">
          <span>
            {s.t("templates.document.selected")} <strong>“{selection.text}”</strong>
          </span>
          <Button icon={<Tag />} onClick={() => open("field")}>
            {s.t("templates.document.makeField")}
          </Button>
          <Button variant="secondary" icon={<Pencil />} onClick={() => open("text")}>
            {s.t("templates.document.changeText")}
          </Button>
          <Button variant="ghost" icon={<X />} onClick={() => setSelection(null)}>
            {s.t("action.cancel")}
          </Button>
        </div>
      )}
      {error && <Notice kind="error">{error}</Notice>}
      <div className="template-blocks">
        {blocks === null ? (
          <p className="muted">{s.t("templates.document.loading")}</p>
        ) : (
          blocks.map((block) => (
            <p
              key={`${block.part}:${block.index}`}
              className="template-block"
              onMouseUp={(event) => capture(block, event.currentTarget)}
            >
              {block.text.split(PLACEHOLDER).map((piece, at) =>
                at % 2 === 1 ? (
                  <mark key={at} className="template-mark">
                    {piece}
                  </mark>
                ) : (
                  piece
                ),
              )}
            </p>
          ))
        )}
      </div>
      {mode && selection && (
        <Modal
          title={s.t(mode === "field" ? "templates.document.makeField" : "templates.document.changeText")}
          closeLabel={s.t("action.cancel")}
          onClose={() => setMode(null)}
        >
          <div className="template-new">
            <p className="muted">“{selection.text}”</p>
            {mode === "field" ? (
              <>
                <label className="field">
                  <span>{s.t("templates.document.source")}</span>
                  <select value={source} onChange={(event) => setSource(event.target.value)}>
                    <option value="manual">{s.t("templates.document.source.manual")}</option>
                    <optgroup label={s.t("templates.document.source.log")}>
                      {LOG_SOURCES.map(([token, name]) => (
                        <option key={token} value={token}>
                          {name}
                        </option>
                      ))}
                    </optgroup>
                    {fields.length > 0 && (
                      <optgroup label={s.t("templates.document.source.existing")}>
                        {fields.map((field) => (
                          <option key={field.key} value={`existing:${field.key}`}>
                            {field.label || field.key}
                          </option>
                        ))}
                      </optgroup>
                    )}
                  </select>
                </label>
                {!source.startsWith("existing:") && (
                  <label className="field">
                    <span>{s.t("templates.editor.label")}</span>
                    <input
                      value={label}
                      placeholder={
                        LOG_SOURCES.find(([token]) => token === source)?.[1] ?? selection.text
                      }
                      onChange={(event) => setLabel(event.target.value)}
                    />
                  </label>
                )}
              </>
            ) : (
              <label className="field">
                <span>{s.t("templates.document.newText")}</span>
                <textarea
                  rows={3}
                  value={replacement}
                  onChange={(event) => setReplacement(event.target.value)}
                />
              </label>
            )}
            <Button disabled={busy} onClick={() => void submit()}>
              {s.t("templates.document.apply")}
            </Button>
          </div>
        </Modal>
      )}
    </section>
  );
}
