// The customer of licences: a list of every customer, A to Z, that narrows by name (or email) as
// you type and loads more as it scrolls; pick one, or keep typing to create a new one. Leaving it
// empty means no customer.

import { useState } from "react";
import { query } from "../shared/api";
import { Button } from "../shared/components/Button";
import { Combobox } from "../shared/components/Combobox";
import { useDebounced, useInfiniteList } from "../shared/hooks";
import type { Customer } from "../shared/types";

export type CustomerChoice = { kind: "existing"; customer: Customer } | { kind: "new"; name: string; email: string };

const PAGE_SIZE = 50;

const sameName = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/** `name` with the first occurrence of `typed` in bold. */
function Highlighted({ name, typed }: { name: string; typed: string }) {
  const at = typed ? name.toLowerCase().indexOf(typed.toLowerCase()) : -1;
  if (at < 0) return <>{name}</>;
  return (
    <>
      {name.slice(0, at)}
      <strong>{name.slice(at, at + typed.length)}</strong>
      {name.slice(at + typed.length)}
    </>
  );
}

function CustomerList({ choice, onChange }: { choice: { kind: "new"; name: string; email: string }; onChange: (choice: CustomerChoice) => void }) {
  const [open, setOpen] = useState(false);
  const typed = choice.name.trim();
  const search = useDebounced(typed);
  const list = useInfiniteList<Customer>(open ? `/customers${query({ order: "name", q: search })}` : null, PAGE_SIZE);
  const settled = search === typed && !list.loading;
  // A customer with exactly this name comes first, and is the one to pick rather than a duplicate.
  const exact = list.rows.filter((customer) => sameName(customer.name, typed));
  const rows = typed ? [...exact, ...list.rows.filter((customer) => !exact.includes(customer))] : list.rows;
  const offerNew = Boolean(typed) && settled && exact.length === 0;
  function pick(next: CustomerChoice) {
    onChange(next);
    setOpen(false);
  }
  return (
    <Combobox<Customer>
      label="Customer (optional)"
      placeholder="Search customers by name"
      noun="customers"
      text={choice.name}
      onText={(name) => onChange({ ...choice, name })}
      open={open}
      onOpen={setOpen}
      rows={rows}
      rowKey={(customer) => customer.id}
      renderRow={(customer) => (
        <>
          <Highlighted name={customer.name} typed={typed} />
          {customer.email && <span className="muted"> · {customer.email}</span>}
        </>
      )}
      onPick={(customer) => pick({ kind: "existing", customer })}
      extraRow={offerNew ? { key: "new", content: `New customer: "${typed}"`, onPick: () => pick(choice) } : null}
      loading={list.loading}
      error={list.error}
      onRetry={list.retry}
      more={list.more}
      onLoadMore={list.loadMore}
      empty={typed ? "No customer matches" : "No customers yet: type a name to create one."}
    />
  );
}

export function CustomerPicker({ choice, onChange }: { choice: CustomerChoice; onChange: (choice: CustomerChoice) => void }) {
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
      <CustomerList choice={choice} onChange={onChange} />
      {choice.name.trim() && (
        <label className="field">
          <span className="field__label">New customer's email (optional)</span>
          <input type="email" value={choice.email} maxLength={254} onChange={(event) => onChange({ ...choice, email: event.target.value })} />
        </label>
      )}
    </>
  );
}
