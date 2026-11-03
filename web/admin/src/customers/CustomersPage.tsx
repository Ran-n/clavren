// Customers: search, edit in place, create in a new first row, see their licences, and delete
// (which erases their name and email but keeps their licences working).

import { useState } from "react";
import { api, query } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { DataTable, type Column } from "../shared/components/DataTable";
import { InlineText } from "../shared/components/InlineText";
import { FilterSelect, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { formatDate } from "../shared/format";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import type { Customer } from "../shared/types";
import { NewCustomerRow } from "./NewCustomerRow";

const ORDER_OPTIONS = [
  { value: "name", label: "A to Z" },
  { value: "newest", label: "Newest first" },
];

function columns(save: (customer: Customer, field: "name" | "email" | "notes", value: string | null) => Promise<void>, onDelete: (id: string) => void): Column<Customer>[] {
  return [
    { key: "name", label: "Name", render: (customer) => <InlineText label="name" value={customer.name} maxLength={200} onSave={(value) => save(customer, "name", value)} /> },
    { key: "email", label: "Email", render: (customer) => <InlineText label="email" type="email" value={customer.email} maxLength={254} placeholder="Add an email" onSave={(value) => save(customer, "email", value)} /> },
    { key: "notes", label: "Notes", render: (customer) => <InlineText label="notes" value={customer.notes} maxLength={2000} placeholder="Add a note" onSave={(value) => save(customer, "notes", value)} /> },
    { key: "added", label: "Added", render: (customer) => formatDate(customer.created_at) },
    {
      key: "actions",
      label: "",
      className: "table__action",
      render: (customer) => (
        <span className="button-row">
          <Link to={`/licences?customer=${customer.id}`} className="button button--quiet button--small">
            Licences
          </Link>
          <Button size="small" variant="quiet" onClick={() => onDelete(customer.id)}>
            Delete
          </Button>
        </span>
      ),
    },
  ];
}

export function CustomersPage() {
  useTitle("Customers");
  const [table, setTable] = useTablePreferences("customers");
  const [search, setSearch] = useState(table.search ?? "");
  const [adding, setAdding] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const q = useDebounced(search);
  const order = table.order === "newest" ? "newest" : "name";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const list = usePagedList<Customer>(`/customers${query({ q, order })}`, pageSize);

  function onSearch(value: string) {
    setSearch(value);
    setTable({ search: value || undefined });
  }

  async function save(customer: Customer, field: "name" | "email" | "notes", value: string | null) {
    const changed = await api<Customer>("PATCH", `/customers/${customer.id}`, { [field]: value });
    list.replace(changed, (existing) => existing.id === changed.id);
  }

  async function remove(id: string) {
    await api("DELETE", `/customers/${id}`);
    setDeleting(null);
    list.reload();
  }

  return (
    <section className="page">
      <h1>Customers</h1>
      <Button variant="primary" size="large" className="page__primary" onClick={() => setAdding(true)} disabledReason={adding ? "Fill in the new row first" : null}>
        New customer
      </Button>
      <Toolbar>
        <SearchBox label="Search by name or email" value={search} onChange={onSearch} />
        <FilterSelect label="Order" value={order} options={ORDER_OPTIONS} onChange={(value) => setTable({ order: value })} />
      </Toolbar>
      <ErrorNotice error={list.error} />
      <DataTable
        label="Customers"
        rows={list.rows}
        columns={columns(save, setDeleting)}
        rowKey={(customer) => customer.id}
        loading={list.loading}
        firstRow={adding ? <NewCustomerRow onCreated={() => { setAdding(false); list.reload(); }} onCancel={() => setAdding(false)} /> : null}
        below={(customer) =>
          deleting === customer.id ? (
            <ConfirmStrip
              message={`Delete ${customer.name}? Their licences keep working; their name and email are erased.`}
              confirmLabel="Delete"
              onConfirm={() => remove(customer.id)}
              onCancel={() => setDeleting(null)}
            />
          ) : null
        }
        empty={q ? "No customer matches." : "No customers yet. They are added here or when issuing licences."}
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
