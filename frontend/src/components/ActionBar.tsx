import type { ReactNode } from "react";

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
