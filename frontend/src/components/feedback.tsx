import { CheckCircle2, CircleAlert } from "lucide-react";
import type { ReactNode } from "react";

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

export function Centered({ children }: { children: ReactNode }) {
  return <main className="centered">{children}</main>;
}
