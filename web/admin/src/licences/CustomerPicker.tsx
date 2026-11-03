// The customer of new licences: type a name to find an existing customer, or keep typing to
// create a new one with the licences. Leaving it empty issues them without a customer.

import { useEffect, useState } from "react";
import { get, query } from "../shared/api";
import { Button } from "../shared/components/Button";
import { useDebounced } from "../shared/hooks";
import type { Customer, Page } from "../shared/types";

export type CustomerChoice = { kind: "existing"; customer: Customer } | { kind: "new"; name: string; email: string };

const SUGGESTIONS = 5;

export function CustomerPicker({ choice, onChange }: { choice: CustomerChoice; onChange: (choice: CustomerChoice) => void }) {
  const typed = choice.kind === "new" ? choice.name.trim() : "";
  const search = useDebounced(typed);
  const [matches, setMatches] = useState<Customer[]>([]);

  useEffect(() => {
    if (search.length < 2) return setMatches([]);
    let current = true;
    get<Page<Customer>>(`/customers${query({ q: search, limit: SUGGESTIONS })}`).then(
      (page) => current && setMatches(page.data),
      () => current && setMatches([]), // suggestions are a convenience; typing a new customer still works
    );
    return () => {
      current = false;
    };
  }, [search]);

  if (choice.kind === "existing") {
    return (
      <div className="field">
        <span className="field__label">Customer</span>
        <span className="chosen">
          <span>
            {choice.customer.name}
            {choice.customer.email && <span className="muted"> · {choice.customer.email}</span>}
          </span>
          <Button size="small" variant="quiet" onClick={() => onChange({ kind: "new", name: "", email: "" })}>
            Change
          </Button>
        </span>
      </div>
    );
  }
  return (
    <>
      <label className="field">
        <span className="field__label">Customer name (optional)</span>
        <input value={choice.name} maxLength={200} autoComplete="off" onChange={(event) => onChange({ ...choice, name: event.target.value })} />
        {matches.length > 0 && (
          <span className="suggestions" role="list" aria-label="Existing customers">
            {matches.map((customer) => (
              <button key={customer.id} type="button" role="listitem" className="suggestion" onClick={() => onChange({ kind: "existing", customer })}>
                {customer.name}
                {customer.email && <span className="muted"> · {customer.email}</span>}
              </button>
            ))}
          </span>
        )}
      </label>
      {choice.name.trim() && (
        <label className="field">
          <span className="field__label">New customer's email (optional)</span>
          <input type="email" value={choice.email} maxLength={254} onChange={(event) => onChange({ ...choice, email: event.target.value })} />
        </label>
      )}
    </>
  );
}
