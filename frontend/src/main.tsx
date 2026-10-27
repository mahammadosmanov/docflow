import { createRoot } from "react-dom/client";

import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { loadLocale, systemLocale } from "./i18n";
import "./styles.css";

void loadLocale(systemLocale()).then(() =>
  createRoot(document.getElementById("root")!).render(
    <ErrorBoundary>
      <App />
    </ErrorBoundary>,
  ),
);
