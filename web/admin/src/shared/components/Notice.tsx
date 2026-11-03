// Messages in the page: errors (with what to do), successes, and the loading state.

import type { ReactNode } from "react";
import { describe } from "../errors";

export function ErrorNotice({ error, children }: { error: unknown; children?: ReactNode }) {
  if (!error) return null;
  return (
    <div className="notice notice--error" role="alert">
      <span>{describe(error)}</span>
      {children}
    </div>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "success" | "warning"; children: ReactNode }) {
  return (
    <div className={`notice notice--${tone}`} role={tone === "warning" ? "alert" : "status"}>
      {children}
    </div>
  );
}

export function Loading({ label = "Loading…" }: { label?: string }) {
  return (
    <p className="loading" role="status">
      <span className="loading__spinner" aria-hidden="true" />
      {label}
    </p>
  );
}
