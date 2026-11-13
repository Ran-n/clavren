// Issuing licences, inline at the top of the Licences page: product, customer (an existing one,
// a new one typed in place, or none), quantity, optional expiry, notes and price per key (the
// product's, changeable; none for test licences), and whether they are test licences (never remembered, so the next ones aren't by mistake). The new keys stay on
// screen only until the panel closes, and are copied to the clipboard as they arrive (the copy
// starts at the Issue press, which browsers require), with Copy key(s) and, for several, a Copy
// per key. Copying them is not audited: the issue already is, and the keys came in its answer.

import { useEffect, useState, type FormEvent } from "react";
import { api, download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { CopyButton, copyFailure, copyText } from "../shared/components/Copy";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import { PriceInput, priceText, useDefaultCurrency } from "../shared/components/PriceField";
import { fromDateInput, parsePrice } from "../shared/format";
import type { Customer, Licence, Product } from "../shared/types";
import type { IssueDraft } from "../shared/preferences";
import { CustomerPicker, type CustomerChoice } from "./CustomerPicker";
import { MAX_BATCH, csvName, customerBody } from "./licenceApi";

type Issued = { licences: (Licence & { key: string })[]; customer: Customer | null; message: string | null };
/** How copying the new keys went: copied, the reason it failed, or still under way (`null`). */
type Copied = "copied" | { failed: string } | null;

/** The new keys, one per line, in the order shown. */
const keysText = (issued: Issued) => issued.licences.map((licence) => licence.key).join("\n");
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

function CopiedNotice({ copied, count }: { copied: Copied; count: number }) {
  if (copied === null) return null;
  if (copied === "copied") return <Notice tone="success">{count === 1 ? "The key is on the clipboard." : `The ${count} keys are on the clipboard.`}</Notice>;
  return <Notice tone="warning">{copied.failed}</Notice>;
}

function IssuedKeys({ issued, copied, onMore, onClose }: { issued: Issued; copied: Copied; onMore: () => void; onClose: () => void }) {
  const [error, setError] = useState<unknown>(null);
  const ids = issued.licences.map((licence) => licence.id);
  const count = issued.licences.length;
  return (
    <div className="issue-result">
      <h2>
        {plural(issued.licences.length, issued.licences[0]?.is_test ? "test licence" : "licence")} issued
      </h2>
      <CopiedNotice copied={copied} count={count} />
      <ul className="issue-result__keys" aria-label="New keys">
        {issued.licences.map((licence) => (
          <li key={licence.id}>
            <code className="key">{licence.key}</code>
            {count > 1 && <CopyButton label="Copy" text={() => Promise.resolve(licence.key)} />}
          </li>
        ))}
      </ul>
      <div className="button-row">
        <CopyButton size="normal" label={count === 1 ? "Copy key" : "Copy keys"} text={() => Promise.resolve(keysText(issued))} />
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
  const [test, setTest] = useState(false);
  const fallbackCurrency = useDefaultCurrency();
  // the price typed for the chosen product; refilled from the product when it changes
  const [price, setPrice] = useState<{ product: string; amount: string; currency: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [issued, setIssued] = useState<Issued | null>(null);
  const [copied, setCopied] = useState<Copied>(null);
  const chosenProduct = product || remembered || active[0]?.id || "";
  const productPrice = active.find((option) => option.id === chosenProduct)?.price ?? null;
  const shownPrice =
    price?.product === chosenProduct ? price : { product: chosenProduct, amount: priceText(productPrice), currency: productPrice?.currency ?? fallbackCurrency };
  const parsedPrice = test ? null : parsePrice(shownPrice.amount, shownPrice.currency);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setCopied(null);
    if (typeof parsedPrice === "string") {
      setBusy(false);
      return;
    }
    try {
      const body = { product: chosenProduct, customer: customerBody(customer), quantity, expires_at: fromDateInput(expiry) ?? undefined, internal_notes: notes.trim() || undefined, test: test || undefined, price: test ? undefined : parsedPrice };
      const request = api<Issued>("POST", "/licences", body);
      // started before the answer is awaited, while the press still counts as the user's; a failed
      // request is shown by the error notice alone, so its copy failure is dropped
      const copying = copyText(request.then(keysText)).then(
        (): Copied => "copied",
        (failure: unknown): Copied => ({ failed: copyFailure(failure) }),
      );
      setIssued(await request);
      onIssued(); // the keys stay on screen, but a later visit starts with the panel closed
      setCopied(await copying);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  if (issued) return <section className="panel" aria-label="Issue licences"><IssuedKeys issued={issued} copied={copied} onMore={() => { setIssued(null); onMore(); }} onClose={onClose} /></section>;
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
        <div className="field">
          <span className="field__label">Price per key (optional)</span>
          <PriceInput
            label="Price per key"
            amount={test ? "" : shownPrice.amount}
            currency={shownPrice.currency}
            disabled={test}
            title={test ? "Test licences aren't sold" : undefined}
            onAmount={(amount) => setPrice({ ...shownPrice, amount })}
            onCurrency={(currency) => setPrice({ ...shownPrice, currency })}
          />
          {typeof parsedPrice === "string" && (
            <span className="inline-error" role="alert">
              {parsedPrice}
            </span>
          )}
        </div>
        <label className="field field--tick">
          <input type="checkbox" checked={test} onChange={(event) => setTest(event.target.checked)} />
          <span>Test licences: they work on any number of computers, and the Overview leaves them out</span>
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
