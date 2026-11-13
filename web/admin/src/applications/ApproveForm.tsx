// Approving an application, inline below its row: the licence's customer (the existing customer
// with the applicant's email, else a new one from their name and email, or anyone else picked),
// an optional expiry, notes and price (empty: an application is usually not a sale). Approving issues the licence; the applicant's application or
// status page collects the key by itself.

import { useEffect, useState, type FormEvent, type KeyboardEvent } from "react";
import { api, get, query } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ErrorNotice } from "../shared/components/Notice";
import { PriceInput, useDefaultCurrency } from "../shared/components/PriceField";
import { fromDateInput, parsePrice } from "../shared/format";
import type { Customer, Page } from "../shared/types";
import { CustomerPicker, type CustomerChoice } from "../licences/CustomerPicker";
import { customerBody } from "../licences/licenceApi";
import { APPLICATION_EXPAND, type Application } from "./applicationApi";

type Props = { application: Application; onApproved: (application: Application) => void; onCancel: () => void };

/** The customer who already has the applicant's email, if any. */
async function existingCustomer(email: string): Promise<Customer | null> {
  const page = await get<Page<Customer>>(`/customers${query({ q: email, limit: 10 })}`);
  return page.data.find((customer) => customer.email?.toLowerCase() === email.toLowerCase()) ?? null;
}

export function ApproveForm({ application, onApproved, onCancel }: Props) {
  const [customer, setCustomer] = useState<CustomerChoice>({ kind: "new", name: application.name, email: application.email });
  const [expiry, setExpiry] = useState("");
  const [notes, setNotes] = useState("");
  const fallbackCurrency = useDefaultCurrency();
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState<string | null>(null);
  const priceCurrency = currency ?? fallbackCurrency;
  const price = parsePrice(amount, priceCurrency);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let current = true;
    existingCustomer(application.email).then(
      (found) => current && found && setCustomer({ kind: "existing", customer: found }),
      // no match found is the default: a new customer from the application
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [application.email]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (typeof price === "string") return;
    setBusy(true);
    setError(null);
    try {
      const body = { customer: customerBody(customer), expires_at: fromDateInput(expiry) ?? undefined, internal_notes: notes.trim() || undefined, price: price ?? undefined };
      onApproved(await api<Application>("POST", `/applications/${application.id}/approve?expand=${APPLICATION_EXPAND}`, body));
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  function onKeyDown(event: KeyboardEvent<HTMLFormElement>) {
    if (event.key === "Escape" && !busy) onCancel();
  }

  return (
    <form className="approve-form" aria-label="Approve application" onSubmit={(event) => void submit(event)} onKeyDown={onKeyDown}>
      <CustomerPicker choice={customer} onChange={setCustomer} />
      <label className="field field--narrow">
        <span className="field__label">Expires (optional)</span>
        <input type="date" value={expiry} onChange={(event) => setExpiry(event.target.value)} />
      </label>
      <label className="field">
        <span className="field__label">Notes (optional)</span>
        <input value={notes} maxLength={2000} onChange={(event) => setNotes(event.target.value)} />
      </label>
      <div className="field">
        <span className="field__label">Price per key (optional)</span>
        <PriceInput label="Price per key" amount={amount} currency={priceCurrency} onAmount={setAmount} onCurrency={setCurrency} />
        {typeof price === "string" && (
          <span className="inline-error" role="alert">
            {price}
          </span>
        )}
      </div>
      <p className="field__help">Approving issues one licence. The applicant collects its key from the application or their status page.</p>
      <ErrorNotice error={error} />
      <span className="button-row">
        <Button type="submit" variant="primary" size="small" busy={busy}>
          Approve and issue
        </Button>
        <Button size="small" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </span>
    </form>
  );
}
