// Issuing licences, inline at the top of the Licences page: product, customer (an existing one,
// a new one typed in place, or none), quantity, optional expiry and notes. The new keys stay on
// screen only until the panel closes.

import { useEffect, useState, type FormEvent } from "react";
import { api, download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import { ErrorNotice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import { fromDateInput } from "../shared/format";
import type { Customer, Licence, Product } from "../shared/types";
import type { IssueDraft } from "../shared/preferences";
import { CustomerPicker, type CustomerChoice } from "./CustomerPicker";
import { MAX_BATCH, csvName, customerBody } from "./licenceApi";

type Issued = { licences: (Licence & { key: string })[]; customer: Customer | null; message: string | null };
type Props = {
  products: Product[];
  /** Whether `products` is the complete list. */
  productsLoaded: boolean;
  /** Why the products couldn't be loaded (the page shows it). */
  productsError: unknown;
  /** The product, quantity and expiry chosen last time (remembered with the preferences). */
  draft: IssueDraft;
  onDraft: (change: Partial<IssueDraft>) => void;
  onClose: () => void;
  onIssued: () => void;
  onMore: () => void;
};

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

export function IssuePanel({ products, productsLoaded, productsError, draft, onDraft, onClose, onIssued, onMore }: Props) {
  const active = products.filter((product) => product.is_active);
  const [product, setProduct] = useState("");
  // a remembered product that was deleted or deactivated since is no choice any more
  const remembered = active.some((option) => option.id === draft.product) ? draft.product : undefined;
  useEffect(() => {
    if (productsLoaded && draft.product && !remembered) onDraft({ product: undefined });
  }, [productsLoaded, draft.product, remembered, onDraft]);
  const [customer, setCustomer] = useState<CustomerChoice>({ kind: "new", name: "", email: "" });
  const [quantity, setQuantity] = useState(draft.quantity ?? 1);
  const [expiry, setExpiry] = useState(draft.expires_at ?? "");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [issued, setIssued] = useState<Issued | null>(null);
  const chosenProduct = product || remembered || active[0]?.id || "";

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const body = { product: chosenProduct, customer: customerBody(customer), quantity, expires_at: fromDateInput(expiry) ?? undefined, internal_notes: notes.trim() || undefined };
      setIssued(await api<Issued>("POST", "/licences", body));
      onIssued(); // the keys stay on screen, but a later visit starts with the panel closed
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  if (issued) return <section className="panel" aria-label="Issue licences"><IssuedKeys issued={issued} onMore={() => { setIssued(null); onMore(); }} onClose={onClose} /></section>;
  return (
    <section className="panel" aria-label="Issue licences">
      <form className="issue-form" onSubmit={(event) => void submit(event)}>
        <h2>Issue licences</h2>
        <label className="field">
          <span className="field__label">Product</span>
          <select value={chosenProduct} onChange={(event) => { setProduct(event.target.value); onDraft({ product: event.target.value }); }} required>
            {active.length === 0 && (
              <option value="">
                {productsError
                  ? "The products couldn't be loaded"
                  : productsLoaded
                    ? "No active product: add one on the Products page"
                    : "Loading products…"}
              </option>
            )}
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
          <input type="number" min={1} max={MAX_BATCH} value={quantity} onChange={(event) => {
              const value = Number(event.target.value);
              setQuantity(value);
              // only a count the server accepts is remembered
              if (Number.isInteger(value) && value >= 1 && value <= MAX_BATCH) onDraft({ quantity: value });
            }} required />
        </label>
        <label className="field field--narrow">
          <span className="field__label">Expires (optional)</span>
          <input type="date" value={expiry} onChange={(event) => { setExpiry(event.target.value); onDraft({ expires_at: event.target.value || undefined }); }} />
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
