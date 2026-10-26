import { type OperationDef, type Plan, type ProgressEvent } from "../types";

export function nextProgressTrail(
  current: ProgressEvent[],
  event: ProgressEvent,
): ProgressEvent[] {
  const next = [...current];
  const last = next.at(-1);
  if (last?.phase === event.phase) next[next.length - 1] = event;
  else next.push(event);
  return next.slice(-8);
}

export function editorRowsOf(plan: Plan): Record<string, unknown>[] {
  const editor = plan.meta?.editor;
  return editor &&
    typeof editor === "object" &&
    Array.isArray((editor as Record<string, unknown>).rows)
    ? ((editor as Record<string, unknown>).rows as Record<string, unknown>[])
    : [];
}

export function rememberedParams(
  operation: OperationDef,
  saved: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(saved).filter(([key]) => {
      const parameter = operation.schema.find((item) => item.name === key);
      return parameter !== undefined && parameter.remember !== false;
    }),
  );
}
