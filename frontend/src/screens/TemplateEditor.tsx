import { Copy, FileUp, History, Plus, Save, Send, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button, Header } from "../components/ui";
import { Notice } from "../components/feedback";
import { type Strings } from "../i18n";
import { type Bridge } from "../lib/bridge";
import { humanError } from "../helpers";
import { Modal } from "../components/Modal";
import { TEMPLATE_DISCIPLINES, withDisciplinePrefix } from "../lib/templateDiscipline";
import { TemplateCells, cellLocation } from "./TemplateCells";
import { TemplateDocument } from "./TemplateDocument";
import {
  type AnchorCheck,
  type TemplateDraft,
  type TemplateFieldMap,
  type TemplateStatus,
  type VersionSummary,
} from "../types";

/** RFI Log values a starting value can use, shown as help. */
const LOG_TOKENS = [
  "rfiNo",
  "itpNo",
  "itemNo",
  "description",
  "forms",
  "tagNo",
  "area",
  "location",
  "dateOfIssue",
  "inspectionDate",
  "startTime",
  "azfenType",
  "intecType",
  "subcontractor",
  "today",
];

const lines = (text: string) =>
  text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

/** Edits one template draft. Only Rust writes it; publishing shares it with everyone. */
export function TemplateEditor({
  bridge,
  s,
  draft: initial,
  onClose,
}: {
  bridge: Bridge;
  s: Strings;
  draft: TemplateDraft;
  onClose: (status: TemplateStatus | null) => void;
}) {
  const [draft, setDraft] = useState(initial);
  const [code, setCode] = useState(initial.code);
  const [title, setTitle] = useState(initial.title);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [discipline, setDiscipline] = useState(initial.discipline);
  const [map, setMap] = useState<TemplateFieldMap>(initial.fieldMap);
  const [optionText, setOptionText] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      initial.fieldMap.fields.map((field) => [field.key, field.options.join("\n")]),
    ),
  );
  const [presetIndex, setPresetIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [checks, setChecks] = useState<AnchorCheck[]>(initial.checks ?? []);
  const [versions, setVersions] = useState<VersionSummary[] | null>(null);
  const [versionsOpen, setVersionsOpen] = useState(false);

  const load = (next: TemplateDraft) => {
    setDraft(next);
    setCode(next.code);
    setTitle(next.title);
    setDiscipline(next.discipline);
    setMap(next.fieldMap);
    setOptionText(
      Object.fromEntries(
        next.fieldMap.fields.map((field) => [field.key, field.options.join("\n")]),
      ),
    );
  };

  const currentMap = (): TemplateFieldMap => ({
    ...map,
    fields: map.fields.map((field) => ({
      ...field,
      options: lines(optionText[field.key] ?? ""),
    })),
    presets: map.presets.map((preset) => ({
      name: preset.name.trim(),
      values: Object.fromEntries(
        Object.entries(preset.values).filter(([, value]) => value !== ""),
      ),
    })),
  });

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      await action();
    } catch (reason) {
      setError(humanError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    const next = await bridge.call("templates/saveDraft", {
      versionId: draft.versionId,
      code: code.trim(),
      title: title.trim(),
      discipline: discipline.trim().toUpperCase(),
      fieldMap: currentMap(),
    });
    load(next);
    setSaved(true);
  };

  const updateField = (index: number, patch: Partial<TemplateFieldMap["fields"][number]>) =>
    setMap((current) => ({
      ...current,
      fields: current.fields.map((field, at) => (at === index ? { ...field, ...patch } : field)),
    }));

  const removeField = (key: string) =>
    setMap((current) => ({
      ...current,
      fields: current.fields.filter((field) => field.key !== key),
      presets: current.presets.map((preset) => {
        const { [key]: _removed, ...values } = preset.values;
        return { ...preset, values };
      }),
    }));

  const toggleSheet = (name: string, optional: boolean) =>
    setMap((current) => {
      const rest = (current.optionalSheets ?? []).filter((item) => item !== name);
      return { ...current, optionalSheets: optional ? [...rest, name] : rest };
    });
  const settle = (key: string) => setChecks((current) => current.filter((item) => item.key !== key));
  const unresolved = checks.filter((item) => item.status !== "found");
  const fieldLabel = (key: string) => {
    const field = map.fields.find((item) => item.key === key);
    return field?.label || key;
  };
  const moveCheck = (check: AnchorCheck) => {
    if (!check.movedTo) return;
    const anchor = check.movedTo;
    setMap((current) => ({
      ...current,
      fields: current.fields.map((field) => (field.key === check.key ? { ...field, anchor } : field)),
    }));
    settle(check.key);
    setSaved(false);
  };
  const openVersions = () =>
    void run(async () => {
      const result = await bridge.call("templates/versions", { code: draft.code });
      setVersions(result.versions);
      setVersionsOpen(true);
    });
  const restoreVersion = (versionId: string) =>
    void run(async () => {
      const next = await bridge.call("templates/restoreVersion", { code: draft.code, versionId });
      load(next);
      setChecks([]);
      setVersionsOpen(false);
    });

  const markCell = (field: TemplateFieldMap["fields"][number]) => {
    setMap((current) => ({ ...current, fields: [...current.fields, field] }));
    setSaved(false);
  };

  const preset = map.presets[presetIndex];
  const setPresets = (presets: TemplateFieldMap["presets"], index: number) => {
    setMap((current) => ({ ...current, presets }));
    setPresetIndex(Math.max(0, Math.min(index, presets.length - 1)));
  };
  const uniqueName = (base: string) => {
    const names = new Set(map.presets.map((item) => item.name.toLowerCase()));
    let name = base;
    for (let n = 2; names.has(name.toLowerCase()); n += 1) name = `${base} ${n}`;
    return name;
  };

  return (
    <div className="screen">
      <Header
        title={s.t("templates.editor.title", { code: draft.code })}
        onBack={() => onClose(null)}
        backLabel={s.t("action.back")}
      />
      <main className="pane template-editor">
        <div className="page-heading compact-heading">
          <div>
            <h1>{code || draft.code}</h1>
            <p>{s.t("templates.editor.draftOf", { version: draft.version })}</p>
          </div>
        </div>

        <section className="content-card template-editor-section">
          <div className="template-editor-grid">
            <label className="field">
              <span>{s.t("templates.editor.discipline")}</span>
              <select
                value={discipline}
                onChange={(event) => {
                  const next = event.target.value;
                  setDiscipline(next);
                  setCode((value) => withDisciplinePrefix(value, next));
                  setTitle((value) => withDisciplinePrefix(value, next));
                }}
              >
                {!TEMPLATE_DISCIPLINES.includes(
                  discipline as (typeof TEMPLATE_DISCIPLINES)[number],
                ) && <option value={discipline}>{discipline}</option>}
                {TEMPLATE_DISCIPLINES.map((item) => (
                  <option key={item} value={item}>
                    {item}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{s.t("templates.editor.code")}</span>
              <input value={code} maxLength={64} onChange={(event) => setCode(event.target.value)} />
            </label>
            <label className="field">
              <span>{s.t("templates.editor.name")}</span>
              <input value={title} onChange={(event) => setTitle(event.target.value)} />
            </label>
            <label className="field">
              <span>{s.t("templates.editor.fileName")}</span>
              <input
                value={map.fileName}
                onChange={(event) => setMap({ ...map, fileName: event.target.value })}
              />
              <small>{s.t("templates.editor.fileName.help")}</small>
            </label>
          </div>
          <div className="template-editor-actions">
            <Button
              variant="secondary"
              icon={<FileUp />}
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const next = await bridge.call("templates/replaceFile", {
                    versionId: draft.versionId,
                  });
                  if (next) {
                    load(next);
                    setChecks(next.checks ?? []);
                  }
                })
              }
            >
              {s.t("templates.editor.replaceFile")}
            </Button>
            <Button variant="secondary" icon={<History />} disabled={busy} onClick={openVersions}>
              {s.t("templates.versions.open")}
            </Button>
            <Button variant="secondary" icon={<Save />} disabled={busy} onClick={() => void run(save)}>
              {s.t("templates.editor.save")}
            </Button>
            <Button
              icon={<Send />}
              disabled={busy || unresolved.length > 0}
              onClick={() =>
                void run(async () => {
                  await save();
                  onClose(await bridge.call("templates/publish", { versionId: draft.versionId }));
                })
              }
            >
              {s.t("templates.editor.publish")}
            </Button>
            <Button
              variant="danger"
              icon={<Trash2 />}
              disabled={busy}
              onClick={() => setConfirmDelete(true)}
            >
              {s.t("templates.editor.delete")}
            </Button>
          </div>
          {unresolved.length > 0 && (
            <Notice kind="info">{s.t("templates.check.blocked")}</Notice>
          )}
          {saved && <Notice kind="info">{s.t("templates.editor.saved")}</Notice>}
          {error && <Notice kind="error">{error}</Notice>}
        </section>

        {checks.length > 0 && (
          <section className="content-card template-editor-section">
            <h2>{s.t("templates.check.title")}</h2>
            <p className="muted">{s.t("templates.check.help")}</p>
            {checks.some((item) => item.status === "found") && (
              <p className="muted">
                {s.t("templates.check.found", {
                  count: checks.filter((item) => item.status === "found").length,
                })}
              </p>
            )}
            {unresolved.map((check) => (
              <div className="template-field" key={check.key}>
                <p>
                  {check.status === "moved" && check.movedTo
                    ? s.t("templates.check.moved", {
                        label: fieldLabel(check.key),
                        where: cellLocation(s, check.movedTo),
                      })
                    : s.t("templates.check.missing", { label: fieldLabel(check.key) })}
                </p>
                <div className="template-editor-actions">
                  {check.status === "moved" && check.movedTo ? (
                    <Button onClick={() => moveCheck(check)}>{s.t("templates.check.move")}</Button>
                  ) : (
                    <Button variant="secondary" onClick={() => settle(check.key)}>
                      {s.t("templates.check.keep")}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    icon={<Trash2 />}
                    onClick={() => {
                      removeField(check.key);
                      settle(check.key);
                    }}
                  >
                    {s.t("templates.editor.removeField")}
                  </Button>
                </div>
              </div>
            ))}
          </section>
        )}

        <TemplateDocument
          bridge={bridge}
          s={s}
          draft={draft}
          fields={map.fields}
          beforeEdit={save}
          onEdited={(next) => {
            load(next);
            setSaved(true);
          }}
        />

        <TemplateCells
          bridge={bridge}
          s={s}
          versionId={draft.versionId}
          fields={map.fields}
          reloadKey={draft.placeholders.join("|")}
          onMark={markCell}
          onRemove={removeField}
          optionalSheets={map.optionalSheets ?? []}
          onToggleSheet={toggleSheet}
        />

        <section className="content-card template-editor-section">
          <h2>{s.t("templates.editor.fields")}</h2>
          <p className="muted">
            {s.t("templates.editor.fields.help", {
              tokens: LOG_TOKENS.map((token) => `{${token}}`).join(" "),
            })}
          </p>
          {map.fields.length === 0 && (
            <p className="muted">{s.t("templates.editor.noFields")}</p>
          )}
          {map.fields.map((field, index) => {
            const inFile = field.anchor !== undefined || draft.placeholders.includes(field.key);
            return (
              <div className="template-field" key={field.key}>
                <div className="template-field-head">
                  {field.anchor ? (
                    <>
                      <strong>{cellLocation(s, field.anchor)}</strong>
                      <Button
                        variant="ghost"
                        icon={<Trash2 />}
                        onClick={() => removeField(field.key)}
                      >
                        {s.t("templates.editor.removeField")}
                      </Button>
                    </>
                  ) : (
                    <code>{`{{${field.key}}}`}</code>
                  )}
                  {!inFile && (
                    <>
                      <span className="status-label warning">
                        {s.t("templates.editor.notInFile")}
                      </span>
                      <Button
                        variant="ghost"
                        icon={<Trash2 />}
                        onClick={() => removeField(field.key)}
                      >
                        {s.t("templates.editor.removeField")}
                      </Button>
                    </>
                  )}
                </div>
                <div className="template-editor-grid">
                  <label className="field">
                    <span>{s.t("templates.editor.label")}</span>
                    <input
                      value={field.label}
                      onChange={(event) => updateField(index, { label: event.target.value })}
                    />
                  </label>
                  <label className="field">
                    <span>{s.t("templates.editor.value")}</span>
                    {field.multiline ? (
                      <textarea
                        rows={3}
                        value={field.value}
                        onChange={(event) => updateField(index, { value: event.target.value })}
                      />
                    ) : (
                      <input
                        value={field.value}
                        onChange={(event) => updateField(index, { value: event.target.value })}
                      />
                    )}
                  </label>
                  <label className="field">
                    <span>{s.t("templates.editor.options")}</span>
                    <textarea
                      rows={2}
                      value={optionText[field.key] ?? ""}
                      onChange={(event) =>
                        setOptionText({ ...optionText, [field.key]: event.target.value })
                      }
                    />
                  </label>
                </div>
                <label className="toggle-inline">
                  <input
                    type="checkbox"
                    checked={field.multiline}
                    onChange={(event) => updateField(index, { multiline: event.target.checked })}
                  />
                  {s.t("templates.editor.multiline")}
                </label>
              </div>
            );
          })}
        </section>

        <section className="content-card template-editor-section">
          <h2>{s.t("templates.editor.presets")}</h2>
          <p className="muted">{s.t("templates.editor.presets.help")}</p>
          <div className="category-filter-chips">
            {map.presets.map((item, index) => (
              <button
                key={`${index}-${item.name}`}
                type="button"
                className={`category-chip ${index === presetIndex ? "active" : ""}`}
                aria-pressed={index === presetIndex}
                onClick={() => setPresetIndex(index)}
              >
                {item.name || "—"}
              </button>
            ))}
            <Button
              variant="ghost"
              icon={<Plus />}
              onClick={() =>
                setPresets(
                  [...map.presets, { name: uniqueName(s.t("templates.editor.newPreset")), values: {} }],
                  map.presets.length,
                )
              }
            >
              {s.t("templates.editor.addPreset")}
            </Button>
          </div>
          {preset && (
            <div className="template-preset">
              <div className="template-editor-grid">
                <label className="field">
                  <span>{s.t("templates.editor.presetName")}</span>
                  <input
                    value={preset.name}
                    onChange={(event) =>
                      setPresets(
                        map.presets.map((item, index) =>
                          index === presetIndex ? { ...item, name: event.target.value } : item,
                        ),
                        presetIndex,
                      )
                    }
                  />
                </label>
              </div>
              <div className="template-editor-actions">
                <Button
                  variant="ghost"
                  icon={<Copy />}
                  onClick={() =>
                    setPresets(
                      [...map.presets, { name: uniqueName(preset.name), values: { ...preset.values } }],
                      map.presets.length,
                    )
                  }
                >
                  {s.t("templates.editor.duplicatePreset")}
                </Button>
                <Button
                  variant="danger"
                  icon={<Trash2 />}
                  onClick={() =>
                    setPresets(
                      map.presets.filter((_, index) => index !== presetIndex),
                      presetIndex - 1,
                    )
                  }
                >
                  {s.t("templates.editor.deletePreset")}
                </Button>
              </div>
              <div className="template-editor-grid">
                {map.fields.map((field) => (
                  <label className="field" key={field.key}>
                    <span>{field.label || field.key}</span>
                    <textarea
                      rows={field.multiline ? 3 : 1}
                      placeholder={s.t("templates.editor.unchanged")}
                      value={preset.values[field.key] ?? ""}
                      onChange={(event) =>
                        setPresets(
                          map.presets.map((item, index) =>
                            index === presetIndex
                              ? { ...item, values: { ...item.values, [field.key]: event.target.value } }
                              : item,
                          ),
                          presetIndex,
                        )
                      }
                    />
                  </label>
                ))}
              </div>
            </div>
          )}
        </section>
      </main>
      {versionsOpen && versions && (
        <Modal
          title={s.t("templates.versions.title", { code: draft.code })}
          closeLabel={s.t("action.cancel")}
          onClose={() => setVersionsOpen(false)}
        >
          <div className="template-new">
            {versions[0]?.status === "draft" && (
              <p className="muted">{s.t("templates.versions.replaceWarning")}</p>
            )}
            {versions.map((item) => (
              <div className="template-field-head" key={item.versionId}>
                <strong>{s.t("templates.versions.version", { version: item.version })}</strong>
                <span className="status-label">{s.t(`templates.versions.status.${item.status}`)}</span>
                {item.publishedAt && (
                  <span className="muted">{new Date(item.publishedAt).toLocaleDateString()}</span>
                )}
                {item.status !== "draft" && (
                  <Button
                    variant="secondary"
                    disabled={busy}
                    onClick={() => restoreVersion(item.versionId)}
                  >
                    {s.t("templates.versions.restore")}
                  </Button>
                )}
              </div>
            ))}
          </div>
        </Modal>
      )}
      {confirmDelete && (
        <Modal
          title={s.t("templates.editor.delete.title", { code: draft.code })}
          closeLabel={s.t("action.cancel")}
          onClose={() => setConfirmDelete(false)}
        >
          <div className="template-new">
            <p>{s.t("templates.editor.delete.help")}</p>
            <div className="template-editor-actions">
              <Button variant="secondary" onClick={() => setConfirmDelete(false)}>
                {s.t("action.cancel")}
              </Button>
              <Button
                variant="danger"
                icon={<Trash2 />}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    setConfirmDelete(false);
                    onClose(await bridge.call("templates/archive", { code: draft.code }));
                  })
                }
              >
                {s.t("templates.editor.delete")}
              </Button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
