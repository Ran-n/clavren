// A new product, typed into the first row of the table: its name, and a code unless it should be
// made from the name (the server makes it, so it knows which codes are taken). The rest is edited
// in place afterwards. Enter saves, Escape cancels.

import { useState, type KeyboardEvent } from "react";
import { api } from "../shared/api";
import { Button } from "../shared/components/Button";
import { describe } from "../shared/errors";

export function NewProductRow({ onCreated, onCancel }: { onCreated: () => void; onCancel: () => void }) {
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    // Enter saves too, and must not get past what disables the button
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api("POST", "/products", { name, code });
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
        <input aria-label="New product's name" placeholder="Name" value={name} maxLength={200} onChange={(event) => setName(event.target.value)} autoFocus disabled={busy} />
      </td>
      <td data-label="Code">
        <input aria-label="New product's code" placeholder="Automatic" value={code} maxLength={63} onChange={(event) => setCode(event.target.value)} disabled={busy} />
      </td>
      <td colSpan={4}>
        <span className="button-row">
          <Button size="small" variant="primary" busy={busy} onClick={() => void save()} disabledReason={name.trim() ? null : "Type a name first"}>
            Save
          </Button>
          <Button size="small" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
        </span>
        <span className="field__help">
          Leave the code empty to make it from the name. The code goes into every licence token; it can't change once the product has licences.
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
