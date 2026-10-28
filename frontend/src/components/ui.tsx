import {
  ArrowLeft,
  Check,
} from "lucide-react";
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
} from "react";

export function Header({
  title,
  titleAs = "p",
  onBack,
  backLabel,
  trailing,
  children,
}: {
  title?: string;
  titleAs?: "h1" | "p";
  onBack?: () => void;
  backLabel?: string;
  trailing?: ReactNode;
  children?: ReactNode;
}) {
  const TitleTag = titleAs;
  return (
    <header className={`app-header ${children ? "with-steps" : ""}`}>
      <div className="header-main">
        <div className="header-left">
          {onBack && (
            <IconButton label={backLabel ?? "←"} onClick={onBack}>
              <ArrowLeft />
            </IconButton>
          )}
          {title && <TitleTag className="header-title">{title}</TitleTag>}
        </div>
        <div className="header-actions">{trailing}</div>
      </div>
      {children}
    </header>
  );
}

export function StepIndicator({
  current,
  labels,
}: {
  current: number;
  labels: string[];
}) {
  return (
    <div className="step-indicator" role="list">
      {labels.map((label, index) => (
        <div
          role="listitem"
          aria-current={index === current ? "step" : undefined}
          className={`${index === current ? "current" : ""} ${
            index < current ? "done" : ""
          }`}
          key={label}
        >
          <span>{index < current ? <Check /> : index + 1}</span>
          <small>{label}</small>
        </div>
      ))}
    </div>
  );
}

export function Button({
  children,
  icon,
  variant = "primary",
  wide,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  wide?: boolean;
}) {
  return (
    <button
      type="button"
      className={`button ${variant} ${wide ? "wide" : ""}`}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="icon-button"
      title={label}
      aria-label={label}
      {...props}
    >
      {children}
    </button>
  );
}

export function Field({
  label,
  help,
  icon,
  suffix,
  invalid,
  onChange,
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, "onChange"> & {
  label: string;
  help?: string;
  icon?: ReactNode;
  suffix?: ReactNode;
  invalid?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className={`field ${invalid ? "invalid" : ""}`}>
      <span>{label}</span>
      <div className="input-shell">
        {icon}
        <input
          aria-label={label}
          {...props}
          onChange={(event) => onChange(event.target.value)}
        />
        {suffix}
      </div>
      {help && <small>{help}</small>}
    </label>
  );
}

export function Switch({
  checked,
  onChange,
  disabled,
  ariaLabel,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  ariaLabel?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      className={`switch ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

export function Segmented({
  value,
  options,
  onChange,
  small,
  label,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
  small?: boolean;
  label?: string;
}) {
  return (
    <div
      className={`segmented ${small ? "small" : ""}`}
      role="group"
      aria-label={label}
    >
      {options.map(([key, text]) => (
        <button
          type="button"
          className={key === value ? "selected" : ""}
          aria-pressed={key === value}
          key={key}
          onClick={() => onChange(key)}
        >
          {text}
        </button>
      ))}
    </div>
  );
}

