// A licence's customer, edited in place: "Add customer" or "Change" swaps the line for the
// customer picker with Save and Cancel (Escape cancels); "Remove customer" unlinks the licence and
// keeps the customer, who may have other licences. A picker has no one value to save on Enter or
// on leaving it (a new customer is a name and an email), so this editor saves with its button;
// picking the customer the licence already has saves nothing. `compact` is the table-row form: the customer
// and a small "Edit" link that shows on hover, on focus, and always on a phone.

import { useState, type KeyboardEvent } from "react";
import { Button } from "../shared/components/Button";
import { ErrorNotice } from "../shared/components/Notice";
import { useFocusOnClose } from "../shared/hooks";
import { expanded, type Licence } from "../shared/types";
import { CustomerPicker, type CustomerChoice } from "./CustomerPicker";
import { customerBody, updateLicence, type CustomerBody } from "./licenceApi";

const NOTHING_CHOSEN: CustomerChoice = { kind: "new", name: "", email: "" };

type Props = { licence: Licence; onChange: (licence: Licence) => void; compact?: boolean };

export function CustomerField({ licence, onChange, compact }: Props) {
  const customer = expanded(licence.customer);
  const [editing, setEditing] = useState(false);
  const [choice, setChoice] = useState<CustomerChoice>(NOTHING_CHOSEN);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const field = useFocusOnClose<HTMLSpanElement>(editing);

  function start() {
    setChoice(NOTHING_CHOSEN);
    setError(null);
    setEditing(true);
  }
  async function save(body: CustomerBody) {
    // the customer it has already (or none, when it has none): nothing to save
    if (body === (customer?.id ?? null)) return setEditing(false);
    setBusy(true);
    setError(null);
    try {
      onChange(await updateLicence(licence.id, { customer: body }));
      setEditing(false);
    } catch (failure) {
      setError(failure); // the editor stays open with what was typed
    } finally {
      setBusy(false);
    }
  }
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape" && !busy) setEditing(false);
  }

  if (!editing) {
    const shown = customer ? (
      compact ? (
        <span className="stacked">
          <span>{customer.name}</span>
          {customer.email && <span className="muted">{customer.email}</span>}
        </span>
      ) : (
        <span>
          {customer.name}
          {customer.email && <span className="muted"> · {customer.email}</span>}
        </span>
      )
    ) : (
      <span className="muted">No customer</span>
    );
    return (
      <span ref={field} className={compact ? "customer-field customer-field--compact" : "customer-field"}>
        {shown}
        {compact ? (
          <button type="button" className="link-button customer-field__edit" onClick={start} aria-label="Edit customer">
            Edit
          </button>
        ) : (
          <Button size="small" variant="quiet" onClick={start}>
            {customer ? "Change" : "Add customer"}
          </Button>
        )}
      </span>
    );
  }
  const body = customerBody(choice);
  return (
    <div className="customer-editor" role="group" aria-label="Customer" onKeyDown={onKeyDown}>
      <CustomerPicker choice={choice} onChange={setChoice} />
      <span className="button-row">
        <Button size="small" variant="primary" busy={busy} disabledReason={body === undefined ? "Pick or type a customer" : null} onClick={() => void save(body ?? null)}>
          Save
        </Button>
        <Button size="small" onClick={() => setEditing(false)} disabled={busy}>
          Cancel
        </Button>
        {customer && (
          <Button size="small" variant="quiet" onClick={() => void save(null)} disabled={busy}>
            Remove customer
          </Button>
        )}
      </span>
      <ErrorNotice error={error} />
    </div>
  );
}
