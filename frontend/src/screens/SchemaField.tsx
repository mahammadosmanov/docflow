import {
  FolderOpen,
  Plus,
  X,
} from "lucide-react";
import {
  useState,
} from "react";
import {
  Button,
  Switch,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Bridge,
} from "../lib/bridge";
import {
  chooseFile,
  chooseFiles,
  chooseFolder,
  dropPaths,
} from "../lib/platform";
import {
  type ParamDef,
} from "../types";
import {
  isPathSelection,
} from "../helpers";
import {
  type PathSelection,
} from "../types";

/** Adds newly chosen files, replacing any already selected file with the same id. */
function mergeSelections(
  current: PathSelection[],
  added: PathSelection[],
): PathSelection[] {
  return [
    ...new Map(
      [...current, ...added].map((item) => [item.selectionId, item]),
    ).values(),
  ];
}

export function SchemaField({
  bridge,
  operationId,
  param,
  value,
  values,
  s,
  onChange,
}: {
  bridge: Bridge;
  operationId: string;
  param: ParamDef;
  value: unknown;
  values: Record<string, unknown>;
  s: Strings;
  onChange: (value: unknown) => void;
}) {
  if (
    !Object.entries(param.visibleWhen ?? {}).every(
      ([name, expected]) => values[name] === expected,
    )
  ) {
    return null;
  }
  if (param.kind === "bool") {
    return (
      <label className="toggle-row">
        <span>
          <strong>{s.t(param.labelKey)}</strong>
          {param.helpKey && <small>{s.t(param.helpKey)}</small>}
        </span>
        <Switch
          checked={Boolean(value)}
          onChange={onChange}
          ariaLabel={s.t(param.labelKey)}
        />
      </label>
    );
  }
  if (param.kind === "choice") {
    const currentChoice = String(value ?? "");
    const choiceIsTemporarilyUnavailable =
      currentChoice.length > 0 && !param.choices.includes(currentChoice);
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <select
          value={currentChoice}
          onChange={(event) => onChange(event.target.value)}
        >
          {choiceIsTemporarilyUnavailable && (
            <option value={currentChoice}>{currentChoice}</option>
          )}
          {param.choices.map((choice) => (
            <option key={choice} value={choice}>
              {param.choiceLabels?.[choice]
                ? s.t(param.choiceLabels[choice])
                : choice}
            </option>
          ))}
        </select>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  if (param.kind === "files") {
    const paths = Array.isArray(value) ? value.filter(isPathSelection) : [];
    const [dragOver, setDragOver] = useState(false);
    return (
      <div
        className={`field dropzone-field ${dragOver ? "drop-active" : ""}`}
        onDragOver={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(true);
        }}
        onDragLeave={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(false);
        }}
        onDrop={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setDragOver(false);
          const dropped = Array.from(event.dataTransfer.files).map(
            (file) => (file as unknown as { path?: string }).path || file.name,
          );
          if (dropped.length > 0) {
            void dropPaths(bridge, operationId, param.name, dropped).then(
              (selected) => {
                onChange(mergeSelections(paths, selected));
              },
            );
          }
        }}
      >
        <span>{s.t(param.labelKey)}</span>
        <div className="multi-file-actions">
          <Button
            type="button"
            variant="secondary"
            icon={<Plus />}
            onClick={() =>
              void chooseFiles(bridge, operationId, param.name).then(
                (selected) => {
                  onChange(mergeSelections(paths, selected));
                },
              )
            }
          >
            {s.t("action.addFiles")}
          </Button>
          {paths.length > 0 && (
            <Button type="button" variant="ghost" onClick={() => onChange([])}>
              {s.t("action.clear")}
            </Button>
          )}
        </div>
        {paths.length > 0 && (
          <div className="selected-files">
            {paths.map((path, index) => (
              <div className="selected-file" key={path.selectionId}>
                <span title={path.displayPath}>{path.name}</span>
                <button
                  type="button"
                  title={s.t("action.remove")}
                  onClick={() =>
                    onChange(
                      paths.filter((_, itemIndex) => itemIndex !== index),
                    )
                  }
                >
                  <X />
                </button>
              </div>
            ))}
          </div>
        )}
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </div>
    );
  }
  if (param.kind === "sheetCounts") {
    const counts =
      value && typeof value === "object"
        ? (value as Record<string, number>)
        : {};
    const update = (choice: string, next: number) =>
      onChange({ ...counts, [choice]: Math.max(0, Math.min(100, next)) });
    const total = param.choices.reduce(
      (sum, choice) => sum + Number(counts[choice] ?? 0),
      0,
    );
    return (
      <div className="field sheet-count-field">
        <span>{s.t(param.labelKey)}</span>
        <div className="sheet-count-grid">
          {param.choices.map((choice) => {
            const count = Number(counts[choice] ?? 0);
            return (
              <div
                className={`sheet-count ${count > 0 ? "active" : ""}`}
                key={choice}
              >
                <strong>{choice}</strong>
                <div>
                  <button
                    type="button"
                    disabled={count <= 0}
                    aria-label={`${choice} ${s.t("action.decrease")}`}
                    onClick={() => update(choice, count - 1)}
                  >
                    −
                  </button>
                  <span>{count}</span>
                  <button
                    type="button"
                    disabled={count >= 100}
                    aria-label={`${choice} ${s.t("action.increase")}`}
                    onClick={() => update(choice, count + 1)}
                  >
                    +
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <small>{s.t("rfi.sheetCounts.total", { count: total })}</small>
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </div>
    );
  }
  const browse = param.kind === "folder" || param.kind === "file";
  const pathValue = isPathSelection(value) ? value.displayPath : "";
  const [browseDragOver, setBrowseDragOver] = useState(false);
  if (param.kind === "longText") {
    return (
      <label className="field">
        <span>{s.t(param.labelKey)}</span>
        <textarea
          aria-label={s.t(param.labelKey)}
          rows={8}
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
        {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      </label>
    );
  }
  return (
    <label className="field">
      <span>{s.t(param.labelKey)}</span>
      <div
        className={`path-input ${browse ? "dropzone-field" : ""} ${browse && browseDragOver ? "drop-active" : ""}`}
        onDragOver={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(true);
              }
            : undefined
        }
        onDragLeave={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(false);
              }
            : undefined
        }
        onDrop={
          browse
            ? (event) => {
                event.preventDefault();
                event.stopPropagation();
                setBrowseDragOver(false);
                const dropped = Array.from(event.dataTransfer.files).map(
                  (file) =>
                    (file as unknown as { path?: string }).path || file.name,
                );
                if (dropped.length > 0) {
                  void dropPaths(bridge, operationId, param.name, dropped).then(
                    (selected) => {
                      if (selected[0]) onChange(selected[0]);
                    },
                  );
                }
              }
            : undefined
        }
      >
        <input
          type={
            param.kind === "int"
              ? "number"
              : param.kind === "secret"
                ? "password"
                : "text"
          }
          value={browse ? pathValue : String(value ?? "")}
          min={param.kind === "int" ? param.min : undefined}
          max={param.kind === "int" ? param.max : undefined}
          placeholder={browse ? s.t(`param.hint.${param.kind}`) : ""}
          readOnly={browse}
          onChange={(event) =>
            !browse &&
            onChange(
              param.kind === "int"
                ? Number(event.target.value)
                : event.target.value,
            )
          }
        />
        {browse && (
          <Button
            type="button"
            variant="secondary"
            icon={<FolderOpen />}
            aria-label={`${s.t("action.browse")}: ${s.t(param.labelKey)}`}
            onClick={() =>
              void (
                param.kind === "folder"
                  ? chooseFolder(bridge, operationId, param.name)
                  : chooseFile(bridge, operationId, param.name)
              ).then((path) => {
                if (path) onChange(path);
              })
            }
          >
            {s.t("action.browse")}
          </Button>
        )}
        {browse && Boolean(pathValue) && (
          <button
            type="button"
            className="field-action"
            title={s.t("action.clear")}
            aria-label={s.t("action.clear")}
            onClick={() => onChange(null)}
          >
            <X />
          </button>
        )}
      </div>
      {param.helpKey && <small>{s.t(param.helpKey)}</small>}
      {param.kind === "int" &&
        param.min !== undefined &&
        param.max !== undefined && (
          <small>{`${param.min}–${param.max}`}</small>
        )}
    </label>
  );
}
