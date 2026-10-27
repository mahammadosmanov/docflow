import { ArrowDown, ArrowUp, Copy, Trash2 } from "lucide-react";
import { type Strings } from "../i18n";

/** Move, duplicate, and remove buttons at the end of a plan row. */
export function PlanRowActions({
  rowIndex,
  rowCount,
  groupCount,
  reorderable,
  duplicateEnabled,
  onMove,
  onDuplicate,
  onRemove,
  s,
}: {
  rowIndex: number;
  rowCount: number;
  groupCount: number;
  reorderable: boolean;
  duplicateEnabled: boolean;
  onMove: (index: number, direction: -1 | 1) => void;
  onDuplicate: (index: number) => void;
  onRemove: (index: number) => void;
  s: Strings;
}) {
  return (
    <td className="editor-row-actions">
      {reorderable && (
        <>
          <button
            type="button"
            aria-label={s.t("action.moveUp")}
            title={s.t("action.moveUp")}
            disabled={rowIndex === 0}
            onClick={() => onMove(rowIndex, -1)}
          >
            <ArrowUp />
          </button>
          <button
            type="button"
            aria-label={s.t("action.moveDown")}
            title={s.t("action.moveDown")}
            disabled={rowIndex === rowCount - 1}
            onClick={() => onMove(rowIndex, 1)}
          >
            <ArrowDown />
          </button>
        </>
      )}
      {duplicateEnabled && (
        <>
          <button
            type="button"
            title={s.t("rfi.action.duplicate")}
            onClick={() => onDuplicate(rowIndex)}
          >
            <Copy />
          </button>
          <button
            type="button"
            title={s.t("rfi.action.removeDuplicate")}
            disabled={groupCount <= 1}
            onClick={() => onRemove(rowIndex)}
          >
            <Trash2 />
          </button>
        </>
      )}
    </td>
  );
}
