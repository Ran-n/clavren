// The inline editors of the bulk bar: one value (expiry, notes or revocation reason), a price or
// a customer for every chosen licence. Apply runs the change; a failure stays under the editor, which keeps
// what was typed. Escape cancels.

import { useState, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "../shared/components/Button";
import { ErrorNotice } from "../shared/components/Notice";
import { PriceInput, useDefaultCurrency } from "../shared/components/PriceField";
import { parsePrice } from "../shared/format";
import type { Price } from "../shared/types";
import { CustomerPicker, type CustomerChoice } from "./CustomerPicker";
import { customerBody, type CustomerBody } from "./licenceApi";

export type ValueField = "expiry" | "notes" | "reason";

const VALUE_FIELDS: Record<ValueField, { label: string; type: "date" | "text"; maxLength: number }> = {
  expiry: { label: "New expiry (empty: permanent)", type: "date", maxLength: 10 },
  notes: { label: "New notes (empty: none)", type: "text", maxLength: 2000 },
  reason: { label: "New reason (empty: none)", type: "text", maxLength: 500 },
};

/** Runs `action`, keeping its failure for the editor to show. */
function useApply(): [boolean, unknown, (action: () => Promise<void>) => Promise<void>] {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function apply(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      setError(failure);
      setBusy(false);
    }
  }
  return [busy, error, apply];
}

function Editor({ busy, error, onCancel, children }: { busy: boolean; error: unknown; onCancel: () => void; children: ReactNode }) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => event.key === "Escape" && !busy && onCancel();
  return (
    <div className="bulk-edit" onKeyDown={onKeyDown}>
      {children}
      <Button size="small" onClick={onCancel} disabled={busy}>
        Cancel
      </Button>
      <ErrorNotice error={error} />
    </div>
  );
}

export function ValueEditor({ field, onApply, onCancel }: { field: ValueField; onApply: (value: string) => Promise<void>; onCancel: () => void }) {
  const [value, setValue] = useState("");
  const [busy, error, apply] = useApply();
  const { label, type, maxLength } = VALUE_FIELDS[field];
  return (
    <Editor busy={busy} error={error} onCancel={onCancel}>
      <label className="field field--inline">
        <span className="field__label">{label}</span>
        <input type={type} value={value} maxLength={maxLength} onChange={(event) => setValue(event.target.value)} autoFocus />
      </label>
      <Button size="small" variant="primary" busy={busy} onClick={() => void apply(() => onApply(value))}>
        Apply
      </Button>
    </Editor>
  );
}

export function PriceEditor({ onApply, onCancel }: { onApply: (price: Price | null) => Promise<void>; onCancel: () => void }) {
  const fallback = useDefaultCurrency();
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState<string | null>(null);
  const [busy, error, apply] = useApply();
  const price = parsePrice(amount, currency ?? fallback);
  return (
    <Editor busy={busy} error={error} onCancel={onCancel}>
      <span className="field field--inline">
        <span className="field__label">New price (empty: none)</span>
        <PriceInput label="New price" amount={amount} currency={currency ?? fallback} onAmount={setAmount} onCurrency={setCurrency} />
      </span>
      <Button size="small" variant="primary" busy={busy} disabledReason={typeof price === "string" ? price : null} onClick={() => void apply(() => onApply(typeof price === "string" ? null : price))}>
        Apply
      </Button>
    </Editor>
  );
}

export function CustomerEditor({ onApply, onCancel }: { onApply: (customer: CustomerBody) => Promise<void>; onCancel: () => void }) {
  const [choice, setChoice] = useState<CustomerChoice>({ kind: "new", name: "", email: "" });
  const [busy, error, apply] = useApply();
  const body = customerBody(choice);
  return (
    <Editor busy={busy} error={error} onCancel={onCancel}>
      <div className="bulk-edit__customer">
        <CustomerPicker choice={choice} onChange={setChoice} />
      </div>
      <Button size="small" variant="primary" busy={busy} disabledReason={body === undefined ? "Pick or type a customer" : null} onClick={() => void apply(() => onApply(body ?? null))}>
        Apply
      </Button>
      <Button size="small" variant="quiet" disabled={busy} onClick={() => void apply(() => onApply(null))}>
        No customer
      </Button>
    </Editor>
  );
}
