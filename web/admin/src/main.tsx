import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./shared/components/ErrorBoundary";
import { applyLastTheme } from "./shared/themeStorage";
import "./shared/theme.css";
import "./shared/styles/layout.css";
import "./shared/styles/buttons.css";
import "./shared/styles/forms.css";
import "./shared/styles/notices.css";
import "./shared/styles/toolbar.css";
import "./shared/styles/tables.css";
import "./shared/styles/inline-editing.css";
import "./shared/styles/confirm-strip.css";
import "./shared/styles/badges-stats.css";
import "./shared/styles/licences.css";
import "./shared/styles/bulk-bar.css";
import "./shared/styles/customer-field.css";
import "./shared/styles/pages.css";
import "./shared/styles/responsive.css";
import "./shared/styles/approve-form.css";

applyLastTheme();

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("index.html is missing the #root element");
}

createRoot(rootElement).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
