import {
  ArrowLeft,
  Check,
  CheckCircle2,
  CircleAlert,
  LoaderCircle,
  X,
} from "lucide-react";
import { useEffect, useId, useRef } from "react";
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
} from "react";

import type { Strings } from "../i18n";
import type { ProgressEvent } from "../types";

export function Header({
  title,
  onBack,
  backLabel,
  trailing,
  children,
}: {
  title?: string;
  onBack?: () => void;
  backLabel?: string;
  trailing?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className={`app-header ${children ? "with-steps" : ""}`}>
      <div className="header-main">
        <div className="header-left">
          {onBack && (
            <IconButton label={backLabel ?? "←"} onClick={onBack}>
              <ArrowLeft />
            </IconButton>
          )}
          <h1>{title}</h1>
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
    <button className={`button ${variant} ${wide ? "wide" : ""}`} {...props}>
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

export function Notice({
  kind,
  title,
  children,
}: {
  kind: "error" | "warning" | "info";
  title?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={`notice ${kind}`}
      role={kind === "info" ? "status" : "alert"}
    >
      {kind === "info" ? <CheckCircle2 /> : <CircleAlert />}
      <div>
        {title && <strong>{title}</strong>}
        <div>{children}</div>
      </div>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  message,
  action,
}: {
  icon: ReactNode;
  title: string;
  message?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <span>{icon}</span>
      <h2>{title}</h2>
      {message && <p>{message}</p>}
      {action}
    </div>
  );
}

export function BusyState({
  label,
  progress,
  progressTrail = [],
  s,
}: {
  label: string;
  progress?: ProgressEvent | null;
  progressTrail?: ProgressEvent[];
  s: Strings;
}) {
  const fraction =
    progress && progress.total > 0 ? progress.done / progress.total : 0;
  return (
    <div className="busy-state">
      <LoaderCircle className="spin" />
      <h2>{label}</h2>
      {progress && (
        <strong className="progress-phase">
          {s.t(`progress.phase.${progress.phase}`)}
        </strong>
      )}
      <div className="progress">
        <span style={{ width: fraction ? `${fraction * 100}%` : "35%" }} />
      </div>
      {progress && (
        <small>
          {progress.total > 0 ? `${progress.done} / ${progress.total}` : ""}
          {progress.subject ? ` · ${progress.subject}` : ""}
        </small>
      )}
      {progressTrail.length > 1 && (
        <ol className="progress-trail" aria-label={s.t("progress.stages")}>
          {progressTrail.map((event, index) => (
            <li
              key={`${event.phase}-${index}`}
              className={index === progressTrail.length - 1 ? "current" : "done"}
            >
              {index === progressTrail.length - 1 ? (
                <LoaderCircle className="spin" aria-hidden="true" />
              ) : (
                <Check aria-hidden="true" />
              )}
              <span>{s.t(`progress.phase.${event.phase}`)}</span>
              {event.total > 0 && (
                <small>
                  {event.done}/{event.total}
                </small>
              )}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

export function ActionBar({
  left,
  right,
}: {
  left: ReactNode;
  right: ReactNode;
}) {
  return (
    <div className="action-bar">
      <div>{left}</div>
      <div>{right}</div>
    </div>
  );
}

export function Centered({ children }: { children: ReactNode }) {
  return <main className="centered">{children}</main>;
}

export function Modal({
  title,
  closeLabel,
  onClose,
  children,
}: {
  title: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    return () => opener?.focus();
  }, []);
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Tab" && dialogRef.current) {
        const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
          "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href]",
        );
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        const active = document.activeElement;
        if (event.shiftKey && (active === first || active === dialogRef.current)) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && active === last) {
          event.preventDefault();
          first?.focus();
        }
        return;
      }
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);
  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <div className="modal-header">
          <h2 id={titleId}>{title}</h2>
          <IconButton label={closeLabel} onClick={onClose}>
            <X />
          </IconButton>
        </div>
        {children}
      </div>
    </div>
  );
}
