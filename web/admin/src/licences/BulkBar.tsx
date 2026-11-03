// What can be done to the selected licences at once: edit expiry, edit notes, export, revoke.
// Every action is one bulk request: all of them change, or none.

import { useEffect, useState } from "react";
import { api, download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { ErrorNotice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import { fromDateInput } from "../shared/format";
import type { Selection } from "../shared/hooks";
import { csvName, filterBody, matchingIds, type LicenceFilters } from "./licenceApi";

type Mode = "idle" | "expiry" | "notes" | "revoke";
type Props = { selection: Selection; filters: LicenceFilters; onDone: () => void };

function EditField({ mode, onApply, onCancel }: { mode: "expiry" | "notes"; onApply: (value: string) => Promise<void>; onCancel: () => void }) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function apply() {
    setBusy(true);
    try {
      await onApply(value);
    } catch (failure) {
      setError(failure);
      setBusy(false);
    }
  }
  return (
    <div className="bulk-edit" onKeyDown={(event) => event.key === "Escape" && onCancel()}>
      <label className="field field--inline">
        <span className="field__label">{mode === "expiry" ? "New expiry (empty: permanent)" : "New notes (empty: none)"}</span>
        <input type={mode === "expiry" ? "date" : "text"} value={value} maxLength={2000} onChange={(event) => setValue(event.target.value)} autoFocus />
      </label>
      <Button size="small" variant="primary" busy={busy} onClick={() => void apply()}>
        Apply
      </Button>
      <Button size="small" onClick={onCancel} disabled={busy}>
        Cancel
      </Button>
      <ErrorNotice error={error} />
    </div>
  );
}

export function BulkBar({ selection, filters, onDone }: Props) {
  const [mode, setMode] = useState<Mode>("idle");
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState<string | null>(null);
  const count = selection.allMatching ? null : selection.ids.size;
  useEffect(() => {
    if (count !== 0) setDone(null); // a new selection replaces the last result
  }, [count]);
  if (count === 0 && !done) return null;
  const what = count === null ? "every licence that matches the filter" : plural(count, "selected licence");

  const ids = () => (selection.allMatching ? matchingIds(filters) : Promise.resolve([...selection.ids]));
  async function finish(message: string) {
    setMode("idle");
    setDone(message);
    selection.clear();
    onDone();
  }
  async function edit(field: "expires_at" | "internal_notes", value: string) {
    const body = { ids: await ids(), [field]: field === "expires_at" ? fromDateInput(value) : value.trim() || null };
    const { updated } = await api<{ updated: number }>("POST", "/licences/bulk/edit", body);
    await finish(`${plural(updated, "licence")} updated.`);
  }
  async function revoke(reason: string | null) {
    const { revoked } = await api<{ revoked: number }>("POST", "/licences/bulk/revoke", { ids: await ids(), reason: reason ?? undefined });
    await finish(`${plural(revoked, "licence")} revoked.`);
  }
  function exportCsv() {
    setError(null);
    const body = selection.allMatching ? { filter: filterBody(filters) } : { ids: [...selection.ids] };
    download("/licences/export", body, csvName()).catch(setError);
  }

  // The result sits where the bar was, the same size, so ticking the next row doesn't move the table.
  if (count === 0) {
    return (
      <div className="bulk-bar bulk-bar--done" role="status">
        {done}
      </div>
    );
  }
  return (
    <div className="bulk-bar" role="region" aria-label="Selected licences">
      <span className="bulk-bar__count">{count === null ? "All matching licences" : plural(count, "licence")} selected</span>
      {mode === "idle" && (
        <span className="button-row">
          <Button size="small" onClick={() => setMode("expiry")}>Edit expiry</Button>
          <Button size="small" onClick={() => setMode("notes")}>Edit notes</Button>
          <Button size="small" onClick={exportCsv}>Export CSV</Button>
          <Button size="small" variant="danger" onClick={() => setMode("revoke")}>Revoke</Button>
          <Button size="small" variant="quiet" onClick={selection.clear}>Clear selection</Button>
        </span>
      )}
      {(mode === "expiry" || mode === "notes") && (
        <EditField mode={mode} onCancel={() => setMode("idle")} onApply={(value) => edit(mode === "expiry" ? "expires_at" : "internal_notes", value)} />
      )}
      {mode === "revoke" && (
        <ConfirmStrip
          message={`Revoke ${what}? Their keys are deleted and can never be used again; computers using them lock at their next check. This can't be undone.`}
          confirmLabel="Revoke"
          withReason
          onConfirm={revoke}
          onCancel={() => setMode("idle")}
        />
      )}
      <ErrorNotice error={error} />
    </div>
  );
}
