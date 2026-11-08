// What can be done to the selected licences at once: set the customer, edit expiry, notes or the
// revocation reason, export, revoke, delete. Every action is one bulk request: all of them change,
// or none. Before a delete the server counts what would go, so the warning is never a guess.

import { useEffect, useState } from "react";
import { api, download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { ErrorNotice } from "../shared/components/Notice";
import { inUseWarning, plural } from "../shared/errors";
import { fromDateInput } from "../shared/format";
import type { Selection } from "../shared/hooks";
import { CustomerEditor, ValueEditor, type ValueField } from "./BulkEditors";
import { csvName, filterBody, matchingIds, type LicenceChanges, type LicenceFilters } from "./licenceApi";

type Mode = "idle" | "customer" | ValueField | "revoke" | "delete";
type Deleted = { deleted: number; in_use: number };

/** The warning before deleting, from the server's count. */
function deleteMessage({ deleted, in_use }: Deleted): string {
  const gone = deleted === 1 ? "Its key stops working and the licence disappears with its history." : "Their keys stop working and the licences disappear with their history.";
  return `Delete ${plural(deleted, "licence")}? ${inUseWarning(in_use)}${gone} This can't be undone.`;
}
type Props = { selection: Selection; filters: LicenceFilters; onDone: () => void };

function valueChange(field: ValueField, value: string): LicenceChanges {
  if (field === "expiry") return { expires_at: fromDateInput(value) };
  const text = value.trim() || null;
  return field === "notes" ? { internal_notes: text } : { revocation_reason: text };
}

export function BulkBar({ selection, filters, onDone }: Props) {
  const [mode, setMode] = useState<Mode>("idle");
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<{ ids: string[]; counts: Deleted } | null>(null);
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
  async function edit(changes: LicenceChanges) {
    const { updated } = await api<{ updated: number }>("POST", "/licences/bulk/edit", { ids: await ids(), ...changes });
    await finish(`${plural(updated, "licence")} updated.`);
  }
  async function revoke(reason: string | null) {
    const { revoked } = await api<{ revoked: number }>("POST", "/licences/bulk/revoke", { ids: await ids(), reason: reason ?? undefined });
    await finish(`${plural(revoked, "licence")} revoked.`);
  }
  async function startDelete() {
    setError(null);
    try {
      const chosen = await ids();
      const counts = await api<Deleted>("POST", "/licences/bulk/delete", { ids: chosen, dry_run: true });
      setDeleting({ ids: chosen, counts });
      setMode("delete");
    } catch (failure) {
      setError(failure); // no warning with guessed counts
    }
  }
  async function remove() {
    const { deleted } = await api<Deleted>("POST", "/licences/bulk/delete", { ids: deleting?.ids ?? [] });
    setDeleting(null);
    await finish(`${plural(deleted, "licence")} deleted.`);
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
          <Button size="small" onClick={() => setMode("customer")}>Set customer</Button>
          <Button size="small" onClick={() => setMode("expiry")}>Edit expiry</Button>
          <Button size="small" onClick={() => setMode("notes")}>Edit notes</Button>
          <Button size="small" onClick={() => setMode("reason")}>Edit reason</Button>
          <Button size="small" onClick={exportCsv}>Export CSV</Button>
          <Button size="small" variant="danger" onClick={() => setMode("revoke")}>Revoke</Button>
          <Button size="small" variant="danger" onClick={() => void startDelete()}>Delete</Button>
          <Button size="small" variant="quiet" onClick={selection.clear}>Clear selection</Button>
        </span>
      )}
      {(mode === "expiry" || mode === "notes" || mode === "reason") && (
        <ValueEditor field={mode} onCancel={() => setMode("idle")} onApply={(value) => edit(valueChange(mode, value))} />
      )}
      {mode === "customer" && <CustomerEditor onCancel={() => setMode("idle")} onApply={(customer) => edit({ customer })} />}
      {mode === "revoke" && (
        <ConfirmStrip
          message={`Revoke ${what}? Their keys are deleted and can never be used again; computers using them lock at their next check. This can't be undone.`}
          confirmLabel="Revoke"
          withReason
          onConfirm={revoke}
          onCancel={() => setMode("idle")}
        />
      )}
      {mode === "delete" && deleting && <ConfirmStrip message={deleteMessage(deleting.counts)} confirmLabel="Delete" onConfirm={remove} onCancel={() => setMode("idle")} />}
      <ErrorNotice error={error} />
    </div>
  );
}
