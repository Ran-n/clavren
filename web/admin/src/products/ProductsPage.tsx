// Products: edit in place, create in a new first row, activate and deactivate (with a warning:
// every computer using it locks at its next check), see their licences, delete unused ones.

import { useState } from "react";
import { api, query } from "../shared/api";
import { Badge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { DataTable, type Column } from "../shared/components/DataTable";
import { InlineText } from "../shared/components/InlineText";
import { FilterSelect, Pager, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { DEFAULT_PAGE_SIZE, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import type { Product } from "../shared/types";
import { NewProductRow } from "./NewProductRow";

type Field = "code" | "name" | "description" | "message_template";
type Pending = { id: string; action: "deactivate" | "delete" } | null;

const ACTIVE_OPTIONS = [
  { value: "", label: "Every product" },
  { value: "true", label: "Active" },
  { value: "false", label: "Inactive" },
];

function columns(save: (product: Product, field: Field, value: string | null) => Promise<void>, setActive: (product: Product, active: boolean) => void, ask: (pending: Pending) => void): Column<Product>[] {
  return [
    { key: "name", label: "Name", render: (product) => <InlineText label="name" value={product.name} maxLength={200} onSave={(value) => save(product, "name", value)} /> },
    { key: "code", label: "Code", render: (product) => <InlineText label="code" value={product.code} maxLength={63} display={<code>{product.code}</code>} onSave={(value) => save(product, "code", value)} /> },
    { key: "description", label: "Description", render: (product) => <InlineText label="description" value={product.description} maxLength={2000} placeholder="Add a description" onSave={(value) => save(product, "description", value)} /> },
    {
      key: "template",
      label: "Message",
      render: (product) => (
        <InlineText label="message template" multiline value={product.message_template} maxLength={10000} display={<span className="clamp">{product.message_template}</span>} onSave={(value) => save(product, "message_template", value)} />
      ),
    },
    { key: "state", label: "State", render: (product) => (product.is_active ? <Badge tone="good">Active</Badge> : <Badge tone="neutral">Inactive</Badge>) },
    {
      key: "actions",
      label: "",
      className: "table__action",
      render: (product) => (
        <span className="button-row">
          <Link to={`/licences?product=${product.id}`} className="button button--quiet button--small">Licences</Link>
          {product.is_active ? (
            <Button size="small" variant="quiet" onClick={() => ask({ id: product.id, action: "deactivate" })}>Deactivate</Button>
          ) : (
            <Button size="small" variant="quiet" onClick={() => setActive(product, true)}>Activate</Button>
          )}
          <Button size="small" variant="quiet" onClick={() => ask({ id: product.id, action: "delete" })}>Delete</Button>
        </span>
      ),
    },
  ];
}

export function ProductsPage() {
  useTitle("Products");
  const [table, setTable] = useTablePreferences("products");
  const [adding, setAdding] = useState(false);
  const [pending, setPending] = useState<Pending>(null);
  const [error, setError] = useState<unknown>(null);
  const active = table.filters?.active ?? "";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const list = usePagedList<Product>(`/products${query({ active })}`, pageSize);

  async function save(product: Product, field: Field | "is_active", value: string | boolean | null) {
    const changed = await api<Product>("PATCH", `/products/${product.id}`, { [field]: value });
    list.replace(changed, (existing) => existing.id === changed.id);
  }
  function setActive(product: Product, isActive: boolean) {
    setError(null);
    save(product, "is_active", isActive).catch(setError);
  }
  async function confirm(product: Product, action: "deactivate" | "delete") {
    if (action === "deactivate") await save(product, "is_active", false);
    else {
      await api("DELETE", `/products/${product.id}`);
      list.reload();
    }
    setPending(null);
  }

  const messages = {
    deactivate: (product: Product) => `Deactivate ${product.name}? Every computer using it locks at its next check, and no new licences can be issued for it.`,
    delete: (product: Product) => `Delete ${product.name}? This only works while it has no licences.`,
  };
  return (
    <section className="page">
      <h1>Products</h1>
      <Button variant="primary" size="large" className="page__primary" onClick={() => setAdding(true)} disabledReason={adding ? "Fill in the new row first" : null}>
        New product
      </Button>
      <Toolbar>
        <FilterSelect label="Show" value={active} options={ACTIVE_OPTIONS} onChange={(value) => setTable({ filters: { active: value } })} />
      </Toolbar>
      <ErrorNotice error={error ?? list.error} />
      <DataTable
        label="Products"
        rows={list.rows}
        columns={columns(save, setActive, setPending)}
        rowKey={(product) => product.id}
        loading={list.loading}
        firstRow={adding ? <NewProductRow onCreated={() => { setAdding(false); list.reload(); }} onCancel={() => setAdding(false)} /> : null}
        below={(product) =>
          pending?.id === product.id ? (
            <ConfirmStrip
              message={messages[pending.action](product)}
              confirmLabel={pending.action === "delete" ? "Delete" : "Deactivate"}
              onConfirm={() => confirm(product, pending.action)}
              onCancel={() => setPending(null)}
            />
          ) : null
        }
        empty="No products yet: add the first one above."
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
