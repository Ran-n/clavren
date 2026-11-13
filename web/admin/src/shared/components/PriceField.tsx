// Prices: an amount typed as a decimal (`19.99` or `19,99`) and a currency picked from the
// browser's list, sent as whole minor units. `PriceInput` is the pair of fields a form holds;
// `InlinePrice` edits a stored price where it is shown (Enter or leaving it saves, Escape
// cancels, an empty amount saves "No price"). The parsing and the look live here only.

import { useEffect, useId, useRef, useState, type FocusEvent, type KeyboardEvent } from "react";
import { get } from "../api";
import { describe } from "../errors";
import { formatPrice, parsePrice } from "../format";
import type { Overview, Price } from "../types";

const FALLBACK_CURRENCY = "EUR";
let defaultCurrency: Promise<string> | null = null;

/** The server's `CLAVREN_CURRENCY`, from the Overview; EUR until it is known. Asked once. */
export function useDefaultCurrency(): string {
  const [currency, setCurrency] = useState(FALLBACK_CURRENCY);
  useEffect(() => {
    defaultCurrency ??= get<Overview>("/overview").then(
      (overview) => overview.default_currency,
      (failure: unknown) => {
        defaultCurrency = null; // asked again next time
        throw failure;
      },
    );
    let current = true;
    defaultCurrency.then(
      (code) => current && setCurrency(code),
      // the preselection is a convenience: without it, EUR stays and the price still saves
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, []);
  return currency;
}

/** Every currency the browser knows, A to Z, with `selected` among them even if it doesn't. */
export function currencies(selected: string): string[] {
  const known = typeof Intl.supportedValuesOf === "function" ? Intl.supportedValuesOf("currency") : [FALLBACK_CURRENCY];
  return known.includes(selected) ? known : [...known, selected].sort();
}

type InputProps = {
  label: string;
  amount: string;
  currency: string;
  onAmount: (amount: string) => void;
  onCurrency: (currency: string) => void;
  disabled?: boolean;
  /** Why it is disabled, shown on hover. */
  title?: string;
  placeholder?: string;
};

/** The amount and currency fields, controlled by the form that holds them. */
export function PriceInput({ label, amount, currency, onAmount, onCurrency, disabled, title, placeholder = "No price" }: InputProps) {
  return (
    <span className="price-input" title={title}>
      <input
        aria-label={label}
        className="price-input__amount"
        inputMode="decimal"
        value={amount}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(event) => onAmount(event.target.value)}
      />
      <select aria-label={`${label} currency`} className="price-input__currency" value={currency} disabled={disabled} onChange={(event) => onCurrency(event.target.value)}>
        {currencies(currency).map((code) => (
          <option key={code} value={code}>
            {code}
          </option>
        ))}
      </select>
    </span>
  );
}

/** The decimal text of a stored price, for editing it. */
export function priceText(price: Price | null): string {
  if (!price) return "";
  const digits = new Intl.NumberFormat("en", { style: "currency", currency: price.currency }).resolvedOptions().maximumFractionDigits ?? 2;
  return (price.amount / 10 ** digits).toFixed(digits);
}

type InlineProps = {
  price: Price | null;
  /** Saves the new price, `null` for none. Rejects to keep the field open with the reason. */
  onSave: (price: Price | null) => Promise<void>;
  label?: string;
  /** Shown instead of an editor (a test licence has no price). */
  readOnly?: string;
};

export function InlinePrice({ price, onSave, label = "price", readOnly }: InlineProps) {
  const valueId = useId();
  const fallback = useDefaultCurrency();
  const [draft, setDraft] = useState<{ amount: string; currency: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finishing = useRef(false);

  if (readOnly !== undefined) return <span className="inline-value__placeholder" title={readOnly}>No price</span>;

  function start() {
    setDraft({ amount: priceText(price), currency: price?.currency ?? fallback });
    setError(null);
    finishing.current = false;
  }
  function cancel() {
    finishing.current = true;
    setDraft(null);
    setError(null);
  }
  async function commit() {
    if (draft === null || finishing.current) return;
    const parsed = parsePrice(draft.amount, draft.currency);
    if (typeof parsed === "string") return setError(parsed);
    if (parsed?.amount === price?.amount && parsed?.currency === price?.currency) return cancel();
    finishing.current = true;
    setSaving(true);
    try {
      await onSave(parsed);
      setDraft(null);
      setError(null);
    } catch (failure) {
      setError(describe(failure));
      finishing.current = false;
    } finally {
      setSaving(false);
    }
  }
  function onKeyDown(event: KeyboardEvent<HTMLSpanElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
    } else if (event.key === "Enter") {
      event.preventDefault();
      void commit();
    }
  }
  /** Leaving the pair saves; moving between the amount and the currency doesn't. */
  function onBlur(event: FocusEvent<HTMLSpanElement>) {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) void commit();
  }

  if (draft === null) {
    return (
      <button type="button" className="inline-value" onClick={start} aria-label={`Edit ${label}`} aria-describedby={valueId}>
        <span id={valueId}>{price ? formatPrice(price) : <span className="inline-value__placeholder">No price</span>}</span>
      </button>
    );
  }
  return (
    <span className="inline-editor" onKeyDown={onKeyDown} onBlur={onBlur}>
      <PriceInputFocused label={label} draft={draft} disabled={saving} onChange={setDraft} />
      {saving && <span className="inline-editor__status">Saving…</span>}
      {error && (
        <span className="inline-editor__error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}

/** [`PriceInput`] with the amount focused as the editor opens. */
function PriceInputFocused({ label, draft, disabled, onChange }: { label: string; draft: { amount: string; currency: string }; disabled: boolean; onChange: (draft: { amount: string; currency: string }) => void }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    ref.current?.querySelector("input")?.focus();
  }, []);
  return (
    <span ref={ref}>
      <PriceInput label={label} amount={draft.amount} currency={draft.currency} disabled={disabled} onAmount={(amount) => onChange({ ...draft, amount })} onCurrency={(currency) => onChange({ ...draft, currency })} />
    </span>
  );
}
