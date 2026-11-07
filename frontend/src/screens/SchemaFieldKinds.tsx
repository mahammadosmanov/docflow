import {
  FolderOpen,
  Plus,
  X,
} from "lucide-react";
import {
  useEffect,
  useEffectEvent,
  useRef,
  useState,
} from "react";
import {
  Button,
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
  listenNativeDrop,
} from "../lib/platform";
import {
  type ParamDef,
  type PathSelection,
} from "../types";
import {
  isPathSelection,
} from "../helpers";

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

/**
 * Highlights the field while an OS file drag is over it and claims the drop
 * when it lands there. Rust holds the dropped paths; the field asks for them.
 */
function useNativeDrop<T extends Element>(enabled: boolean, onDrop: () => void) {
  const ref = useRef<T>(null);
  const [over, setOver] = useState(false);
  const claimDrop = useEffectEvent(onDrop);
  useEffect(() => {
    if (!enabled) return undefined;
    return listenNativeDrop(({ type, target }) => {
      const inside = Boolean(target && ref.current?.contains(target));
      setOver(type === "over" && inside);
      if (type === "drop" && inside) claimDrop();
    });
  }, [enabled]);
  return [ref, over] as const;
}

export function MultiFileField({
  bridge,
  operationId,
  param,
  value,
  s,
  onChange,
}: {
  bridge: Bridge;
  operationId: string;
  param: ParamDef;
  value: unknown;
  s: Strings;
  onChange: (value: unknown) => void;
}) {
  const paths = Array.isArray(value) ? value.filter(isPathSelection) : [];
  const [dropRef, dragOver] = useNativeDrop<HTMLDivElement>(true, () => {
    void dropPaths(bridge, operationId, param.name).then((selected) => {
      onChange(mergeSelections(paths, selected));
    });
  });
  return (
    <div
      ref={dropRef}
      className={`field dropzone-field ${dragOver ? "drop-active" : ""}`}
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

export function PathField({
  bridge,
  operationId,
  param,
  value,
  invalid,
  s,
  onChange,
}: {
  bridge: Bridge;
  operationId: string;
  param: ParamDef;
  value: unknown;
  invalid?: boolean;
  s: Strings;
  onChange: (value: unknown) => void;
}) {
  const browse = param.kind === "folder" || param.kind === "file";
  const pathValue = isPathSelection(value) ? value.displayPath : "";
  const [dropRef, browseDragOver] = useNativeDrop<HTMLDivElement>(browse, () => {
    void dropPaths(bridge, operationId, param.name).then((selected) => {
      if (selected[0]) onChange(selected[0]);
    });
  });
  return (
    <label className="field">
      <span>{s.t(param.labelKey)}</span>
      <div
        ref={dropRef}
        className={`path-input ${browse ? "dropzone-field" : ""} ${browse && browseDragOver ? "drop-active" : ""}`}
      >
        <input
          aria-invalid={invalid || undefined}
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
