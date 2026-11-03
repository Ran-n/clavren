// Issuing licences, inline at the top of the Licences page: product, customer (an existing one,
// a new one typed in place, or none), quantity, optional expiry and notes. The new keys stay on
// screen only until the panel closes.

import { useState, type FormEvent } from "react";
import { api, download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import { ErrorNotice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import { fromDateInput } from "../shared/format";
import type { Customer, Licence, Product } from "../shared/types";
import { CustomerPicker, type CustomerChoice } from "./CustomerPicker";
import { MAX_BATCH, csvName } from "./licenceApi";

type Issued = { licences: (Licence & { key: string })[]; customer: Customer | null; message: string | null };
type Props = { products: Product[]; onClose: () => void; onIssued: () => void };

function customerBody(choice: CustomerChoice): string | { name: string; email?: string } | undefined {
  if (choice.kind === "existing") return choice.customer.id;
  if (choice.kind === "new" && choice.name.trim()) return { name: choice.name.trim(), email: choice.email.trim() || undefined };
  return undefined;
}

function IssuedKeys({ issued, onMore, onClose }: { issued: Issued; onMore: () => void; onClose: () => void }) {
  const [error, setError] = useState<unknown>(null);
  const ids = issued.licences.map((licence) => licence.id);
  return (
    <div className="issue-result">
      <h2>{plural(issued.licences.length, "licence")} issued</h2>
      <ul className="issue-result__keys" aria-label="New keys">
        {issued.licences.map((licence) => (
          <li key={licence.id}>
            <code className="key">{licence.key}</code>
          </li>
        ))}
      </ul>
      <div className="button-row">
        {issued.message && <CopyButton size="normal" label="Copy message" text={() => Promise.resolve(issued.message as string)} />}
        <Button onClick={() => download("/licences/export", { ids }, csvName()).catch(setError)}>Download CSV</Button>
        <Button variant="quiet" onClick={onMore}>
          Issue more
        </Button>
        <Button variant="quiet" onClick={onClose}>
          Close
        </Button>
      </div>
      <ErrorNotice error={error} />
    </div>
  );
}

export function IssuePanel({ products, onClose, onIssued }: Props) {
  const active = products.filter((product) => product.is_active);
  const [product, setProduct] = useState("");
  const [customer, setCustomer] = useState<CustomerChoice>({ kind: "new", name: "", email: "" });
  const [quantity, setQuantity] = useState(1);
  const [expiry, setExpiry] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [issued, setIssued] = useState<Issued | null>(null);
  const chosenProduct = product || active[0]?.id || "";

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const body = { product: chosenProduct, customer: customerBody(customer), quantity, expires_at: fromDateInput(expiry) ?? undefined, internal_notes: notes.trim() || undefined };
      setIssued(await api<Issued>("POST", "/licences", body));
      onIssued();
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  if (issued) return <section className="panel" aria-label="Issue licences"><IssuedKeys issued={issued} onMore={() => setIssued(null)} onClose={onClose} /></section>;
  return (
    <section className="panel" aria-label="Issue licences">
      <form className="issue-form" onSubmit={(event) => void submit(event)}>
        <h2>Issue licences</h2>
        <label className="field">
          <span className="field__label">Product</span>
          <select value={chosenProduct} onChange={(event) => setProduct(event.target.value)} required>
            {active.length === 0 && <option value="">No active product: add one on the Products page</option>}
            {active.map((option) => (
              <option key={option.id} value={option.id}>
                {option.name}
              </option>
            ))}
          </select>
        </label>
        <CustomerPicker choice={customer} onChange={setCustomer} />
        <label className="field field--narrow">
          <span className="field__label">How many</span>
          <input type="number" min={1} max={MAX_BATCH} value={quantity} onChange={(event) => setQuantity(Number(event.target.value))} required />
        </label>
        <label className="field field--narrow">
          <span className="field__label">Expires (optional)</span>
          <input type="date" value={expiry} onChange={(event) => setExpiry(event.target.value)} />
        </label>
        <label className="field">
          <span className="field__label">Notes (optional)</span>
          <input value={notes} maxLength={2000} onChange={(event) => setNotes(event.target.value)} />
        </label>
        <ErrorNotice error={error} />
        <div className="button-row">
          <Button type="submit" variant="primary" busy={busy} disabledReason={chosenProduct ? null : "Add an active product first"}>
            Issue
          </Button>
          <Button variant="quiet" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        </div>
      </form>
    </section>
  );
}
