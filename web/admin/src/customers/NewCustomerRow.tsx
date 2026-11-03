// A new customer, typed into the first row of the table. Enter saves, Escape cancels.

import { useState, type KeyboardEvent } from "react";
import { api } from "../shared/api";
import { Button } from "../shared/components/Button";
import { describe } from "../shared/errors";

export function NewCustomerRow({ onCreated, onCancel }: { onCreated: () => void; onCancel: () => void }) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api("POST", "/customers", { name, email: email.trim() || undefined, notes: notes.trim() || undefined });
      onCreated();
    } catch (failure) {
      setError(describe(failure));
      setBusy(false);
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      void save();
    } else if (event.key === "Escape") {
      onCancel();
    }
  }

  return (
    <tr className="table__new" onKeyDown={onKeyDown}>
      <td data-label="Name">
        <input aria-label="New customer's name" placeholder="Name" value={name} maxLength={200} onChange={(event) => setName(event.target.value)} autoFocus disabled={busy} />
      </td>
      <td data-label="Email">
        <input aria-label="New customer's email" placeholder="Email (optional)" type="email" value={email} maxLength={254} onChange={(event) => setEmail(event.target.value)} disabled={busy} />
      </td>
      <td data-label="Notes">
        <input aria-label="New customer's notes" placeholder="Notes (optional)" value={notes} maxLength={2000} onChange={(event) => setNotes(event.target.value)} disabled={busy} />
      </td>
      <td colSpan={2}>
        <span className="button-row">
          <Button size="small" variant="primary" busy={busy} onClick={() => void save()} disabledReason={name.trim() ? null : "Type a name first"}>
            Save
          </Button>
          <Button size="small" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
        </span>
        {error && (
          <span className="inline-error" role="alert">
            {error}
          </span>
        )}
      </td>
    </tr>
  );
}
