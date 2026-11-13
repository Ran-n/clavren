// The public licence application page: a product's key page at `/apply?product=<code>` (how to get
// a key, and the form when it takes applications), the form with a product picker at `/apply`,
// and an application's private status page at `/apply/status#<secret>`. It follows the system's
// light or dark theme.

import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { ErrorBoundary } from "../shared/components/ErrorBoundary";
import { ApplyForm } from "./ApplyForm";
import { KeyPage } from "./KeyPage";
import { StatusView } from "./StatusView";
import "../shared/theme.css";
import "../shared/components.css";
import "./apply.css";

const STATUS_PATH = "/apply/status";
const productInAddress = () => new URLSearchParams(window.location.search).get("product") ?? "";

function secretInAddress(): string {
  return window.location.pathname === STATUS_PATH ? decodeURIComponent(window.location.hash.slice(1)) : "";
}

function ApplyPage() {
  const [secret, setSecret] = useState(secretInAddress);
  const [justSent, setJustSent] = useState(false);
  useEffect(() => {
    const follow = () => setSecret(secretInAddress());
    window.addEventListener("hashchange", follow);
    window.addEventListener("popstate", follow);
    return () => {
      window.removeEventListener("hashchange", follow);
      window.removeEventListener("popstate", follow);
    };
  }, []);
  const product = window.location.pathname === STATUS_PATH ? "" : productInAddress();
  useEffect(() => {
    // the key page names its product in the title itself
    if (secret || !product) document.title = secret ? "Your licence application" : "Apply for a licence";
  }, [secret, product]);

  function sent(newSecret: string) {
    window.history.pushState(null, "", `${STATUS_PATH}#${newSecret}`);
    setJustSent(true);
    setSecret(newSecret);
  }

  if (!secret && product) {
    return (
      <main className="apply">
        <KeyPage code={product} onSent={sent} />
      </main>
    );
  }
  return (
    <main className="apply">
      <h1>{secret ? "Your licence application" : "Apply for a licence"}</h1>
      {secret ? (
        <StatusView secret={secret} justSent={justSent} />
      ) : window.location.pathname === STATUS_PATH ? (
        <p className="notice notice--error" role="alert">
          This link is incomplete: it should end with # and a long code. Open the whole link you were given.
        </p>
      ) : (
        <ApplyForm onSent={sent} />
      )}
    </main>
  );
}

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("apply.html is missing the #root element");
createRoot(rootElement).render(
  <StrictMode>
    <ErrorBoundary>
      <ApplyPage />
    </ErrorBoundary>
  </StrictMode>,
);
