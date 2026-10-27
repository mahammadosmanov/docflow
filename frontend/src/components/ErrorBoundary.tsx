import { Component, type ErrorInfo, type ReactNode } from "react";

import { makeStrings } from "../i18n";
import { Button } from "./ui";

/** Replaces a crashed screen with a message and reload button instead of a blank window. */
export class ErrorBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  render() {
    if (!this.state.failed) return this.props.children;
    const s = makeStrings(null);
    return (
      <div className="app-crash" role="alert">
        <p>{s.t("error.unexpected")}</p>
        <Button
          type="button"
          variant="secondary"
          onClick={() => window.location.reload()}
        >
          {s.t("action.reload")}
        </Button>
      </div>
    );
  }
}
