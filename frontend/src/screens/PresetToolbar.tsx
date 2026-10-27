import {
  LoaderCircle,
  Save,
  Trash2,
} from "lucide-react";
import {
  useCallback,
  useEffect,
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
  type OperationDef,
  type OperationPreset,
} from "../types";
import {
  humanError,
  isDuplicatePresetError,
} from "../helpers";

type PresetToolbarProps = {
  bridge: Bridge;
  operation: OperationDef;
  values: Record<string, unknown>;
  s: Strings;
  onApply: (preset: OperationPreset) => void;
  onError: (message: string) => void;
};

export function presetCapableValues(
  operation: OperationDef,
  values: Record<string, unknown>,
): Record<string, unknown> {
  const allowed = new Set(
    operation.schema
      .filter(
        (param) =>
          param.remember !== false &&
          (param.kind === "bool" ||
            param.kind === "choice" ||
            param.kind === "int"),
      )
      .map((param) => param.name),
  );
  return Object.fromEntries(
    Object.entries(values).filter(([name]) => allowed.has(name)),
  );
}

export function presetCommandError(error: unknown, s: Strings): string {
  if (
    error &&
    typeof error === "object" &&
    "code" in error &&
    error.code === "settings_write_failed"
  ) {
    return s.t("error.preset.write");
  }
  return humanError(error, s);
}

export function PresetToolbar({
  bridge,
  operation,
  values,
  s,
  onApply,
  onError,
}: PresetToolbarProps) {
  const [presets, setPresets] = useState<OperationPreset[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  const loadPresets = useCallback(async () => {
    setLoading(true);
    try {
      const next = await bridge.call("settings/presets", {
        opId: operation.id,
      });
      setPresets(next);
      return next;
    } catch (reason) {
      onError(presetCommandError(reason, s));
      return null;
    } finally {
      setLoading(false);
    }
  }, [bridge, onError, operation.id, s]);

  useEffect(() => {
    if (!operation.supportsPresets) return;
    void loadPresets();
  }, [loadPresets, operation.supportsPresets]);

  const selected = presets.find((preset) => preset.id === selectedId);
  const applyPreset = () => {
    if (!selected) return;
    onApply({
      ...selected,
      values: presetCapableValues(operation, selected.values),
    });
  };
  const savePreset = async () => {
    const trimmedName = name.trim();
    if (!trimmedName || busy) return;
    setBusy(true);
    setStatus("");
    onError("");
    try {
      const presetValues = presetCapableValues(operation, values);
      let saved: OperationPreset;
      try {
        saved = await bridge.call("settings/savePreset", {
          opId: operation.id,
          name: trimmedName,
          values: presetValues,
        });
      } catch (reason) {
        if (
          !isDuplicatePresetError(reason) ||
          !window.confirm(s.t("preset.confirm.replace", { name: trimmedName }))
        ) {
          throw reason;
        }
        saved = await bridge.call("settings/savePreset", {
          opId: operation.id,
          name: trimmedName,
          values: presetValues,
          replace: true,
        });
      }
      const next = await loadPresets();
      if (next) {
        setSelectedId(saved.id);
        setName("");
        setStatus(s.t("preset.saved", { name: saved.name }));
      }
    } catch (reason) {
      onError(presetCommandError(reason, s));
    } finally {
      setBusy(false);
    }
  };
  const deletePreset = async () => {
    if (
      !selected ||
      busy ||
      !window.confirm(s.t("preset.confirm.delete", { name: selected.name }))
    ) {
      return;
    }
    setBusy(true);
    setStatus("");
    onError("");
    try {
      await bridge.call("settings/deletePreset", {
        opId: operation.id,
        presetId: selected.id,
      });
      const deletedName = selected.name;
      const next = await loadPresets();
      if (next) {
        setSelectedId("");
        setStatus(s.t("preset.deleted", { name: deletedName }));
      }
    } catch (reason) {
      onError(presetCommandError(reason, s));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="preset-toolbar" aria-labelledby="preset-title">
      <div className="preset-toolbar-heading">
        <h3 id="preset-title">{s.t("preset.title")}</h3>
        {!loading && presets.length === 0 && (
          <small>{s.t("preset.empty")}</small>
        )}
      </div>
      <div className="preset-toolbar-row">
        <label className="field preset-select">
          <span>{s.t("preset.select")}</span>
          <select
            value={selectedId}
            disabled={loading || busy}
            onChange={(event) => setSelectedId(event.target.value)}
          >
            <option value="">{s.t("preset.select.placeholder")}</option>
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
        </label>
        <Button
          type="button"
          variant="secondary"
          disabled={!selected || busy}
          onClick={applyPreset}
        >
          {s.t("preset.apply")}
        </Button>
        <Button
          type="button"
          variant="danger"
          icon={<Trash2 />}
          disabled={!selected || busy}
          onClick={deletePreset}
        >
          {s.t("preset.delete")}
        </Button>
      </div>
      <div className="preset-toolbar-row preset-save-row">
        <label className="field preset-name">
          <span>{s.t("preset.name")}</span>
          <input
            type="text"
            value={name}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <Button
          type="button"
          icon={busy ? <LoaderCircle className="spin" /> : <Save />}
          disabled={!name.trim() || busy}
          onClick={savePreset}
        >
          {s.t("preset.save")}
        </Button>
      </div>
      {status && <small role="status">{status}</small>}
    </section>
  );
}
