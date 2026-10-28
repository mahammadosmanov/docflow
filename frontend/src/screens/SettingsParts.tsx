import {
  Check,
} from "lucide-react";
import {
  type ReactNode,
} from "react";

export function SettingsGroup({
  title,
  icon,
  help,
  children,
}: {
  title: string;
  icon: ReactNode;
  help?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-group">
      <div className="settings-title">
        <span>{icon}</span>
        <div>
          <h2>{title}</h2>
          {help && <p>{help}</p>}
        </div>
      </div>
      <div className="settings-content">{children}</div>
    </section>
  );
}

export function ChoiceGrid({
  value,
  options,
  onChange,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="choice-grid">
      {options.map(([key, label]) => (
        <button
          type="button"
          key={key}
          className={value === key ? "selected" : ""}
          onClick={() => onChange(key)}
        >
          <span>{value === key && <Check />}</span>
          {label}
        </button>
      ))}
    </div>
  );
}
