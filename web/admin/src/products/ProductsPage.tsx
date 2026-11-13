// Products: edit in place, create in a new first row, activate and deactivate (with a warning:
// every computer using it locks at its next check), see their licences, delete, with their
// licences (counted by the server first; with licences, the code must be typed to confirm). A new
// product's test licence key is shown until the page is left or the notice dismissed. Licence
// applications are opened and closed per product. Each product's key page (how to get a key, in
// its How to get a key text, and the form when open) has a link to copy, open or closed. Each
// product's offline limit is edited in place; fewer days asks first, as it locks computers sooner.

import { useRef, useState } from "react";
import { api, get, query } from "../shared/api";
import { Badge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { CopyButton } from "../shared/components/Copy";
import { DataTable, type Column } from "../shared/components/DataTable";
import { InlineText } from "../shared/components/InlineText";
import { FilterSelect, Pager, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { inUseWarning, offlineLimit, plural } from "../shared/errors";
import { DEFAULT_PAGE_SIZE, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import type { Price, Product } from "../shared/types";
import { NewProductRow, type CreatedProduct } from "./NewProductRow";
import { OfflineDays } from "./OfflineDays";
import { InlinePrice } from "../shared/components/PriceField";

type Field = "code" | "name" | "description" | "message_template" | "key_instructions";
type Usage = { licences: number; in_use: number };
type Pending =
  | { id: string; action: "deactivate" }
  | { id: string; action: "delete"; usage: Usage }
  | { id: string; action: "offline"; days: number }
  | null;

const ACTIVE_OPTIONS = [
  { value: "", label: "Every product" },
  { value: "true", label: "Active" },
  { value: "false", label: "Inactive" },
];

type Ask = (product: Product, action: "deactivate" | "delete") => void;
const CONFIRM_LABELS = { deactivate: "Deactivate", delete: "Delete", offline: "Limit" } as const;

function TestLicenceNotice({ product, onDismiss }: { product: CreatedProduct; onDismiss: () => void }) {
  const licence = product.test_licence;
  if (!licence) return null;
  return (
    <Notice tone="success">
      <span>
        {product.name} was created with a test licence: <code className="key">{licence.key}</code>
      </span>
      <span className="button-row">
        <CopyButton label="Copy key" text={() => Promise.resolve(licence.key)} />
        <Link to={`/licences/${licence.id}`} className="button button--quiet button--small">
          Open it
        </Link>
        <Button size="small" variant="quiet" onClick={onDismiss}>
          Dismiss
        </Button>
      </span>
    </Notice>
  );
}

/** The product's key page: how to get a key, and the form when it takes applications. */
function keyPageLink(product: Product): string {
  return `${window.location.origin}/apply?product=${encodeURIComponent(product.code)}`;
}

function Applications({ product, onChange }: { product: Product; onChange: (open: boolean) => void }) {
  const copy = <CopyButton label="Copy key page link" text={() => Promise.resolve(keyPageLink(product))} />;
  if (!product.accepts_applications) {
    return (
      <span className="button-row">
        <Badge tone="neutral">Closed</Badge>
        <Button size="small" variant="quiet" onClick={() => onChange(true)}>
          Open
        </Button>
        {copy}
      </span>
    );
  }
  return (
    <span className="button-row">
      <Badge tone="good">Open</Badge>
      <Button size="small" variant="quiet" onClick={() => onChange(false)}>
        Close
      </Button>
      {copy}
    </span>
  );
}

function columns(save: (product: Product, field: Field, value: string | null) => Promise<void>, setActive: (product: Product, active: boolean) => void, ask: Ask, setApplications: (product: Product, open: boolean) => void, setOfflineDays: (product: Product, days: number) => Promise<void>, setPrice: (product: Product, price: Price | null) => Promise<void>): Column<Product>[] {
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
    { key: "price", label: "Price", render: (product) => <InlinePrice label="price per key" price={product.price} onSave={(price) => setPrice(product, price)} /> },
    {
      key: "key_instructions",
      label: "How to get a key",
      render: (product) => (
        <InlineText label="how to get a key" multiline value={product.key_instructions} maxLength={2000} placeholder="Add how to get a key" display={<span className="clamp">{product.key_instructions}</span>} onSave={(value) => save(product, "key_instructions", value)} />
      ),
    },
    { key: "offline", label: "Offline", render: (product) => <OfflineDays days={product.offline_days} onSave={(days) => setOfflineDays(product, days)} /> },
    { key: "state", label: "State", render: (product) => (product.is_active ? <Badge tone="good">Active</Badge> : <Badge tone="neutral">Inactive</Badge>) },
    { key: "applications", label: "Applications", render: (product) => <Applications product={product} onChange={(open) => setApplications(product, open)} /> },
    {
      key: "actions",
      label: "",
      className: "table__action",
      render: (product) => (
        <span className="button-row">
          <Link to={`/licences?product=${product.id}`} className="button button--quiet button--small">Licences</Link>
          {product.is_active ? (
            <Button size="small" variant="quiet" onClick={() => ask(product, "deactivate")}>Deactivate</Button>
          ) : (
            <Button size="small" variant="quiet" onClick={() => setActive(product, true)}>Activate</Button>
          )}
          <Button size="small" variant="quiet" onClick={() => ask(product, "delete")}>Delete</Button>
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
  const [created, setCreated] = useState<CreatedProduct | null>(null);
  // which question about a product is the latest, so a slow answer about another one is dropped
  const asked = useRef(0);
  const [error, setError] = useState<unknown>(null);
  const active = table.filters?.active ?? "";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const list = usePagedList<Product>(`/products${query({ active })}`, pageSize);

  async function save(product: Product, field: Field | "is_active" | "accepts_applications" | "offline_days" | "price", value: string | boolean | number | Price | null) {
    const changed = await api<Product>("PATCH", `/products/${product.id}`, { [field]: value });
    list.replace(changed, (existing) => existing.id === changed.id);
  }
  function setActive(product: Product, isActive: boolean) {
    setError(null);
    save(product, "is_active", isActive).catch(setError);
  }
  function setApplications(product: Product, open: boolean) {
    setError(null);
    save(product, "accepts_applications", open).catch(setError);
  }
  /** More days only give users more time, so they save at once; fewer ask first. */
  async function setOfflineDays(product: Product, days: number) {
    setError(null);
    if (days >= product.offline_days) return save(product, "offline_days", days);
    asked.current += 1;
    setPending({ id: product.id, action: "offline", days });
  }
  /** A delete is counted by the server first; without the count there is no warning to show. */
  async function ask(product: Product, action: "deactivate" | "delete") {
    setError(null);
    const asking = ++asked.current;
    if (action === "deactivate") return setPending({ id: product.id, action });
    try {
      const usage = await get<Usage>(`/products/${product.id}/usage`);
      // another product was picked meanwhile: its question stands
      if (asking === asked.current) setPending({ id: product.id, action, usage });
    } catch (failure) {
      if (asking !== asked.current) return;
      setPending(null);
      setError(failure);
    }
  }
  async function confirm(product: Product, pending: NonNullable<Pending>) {
    if (pending.action === "deactivate") await save(product, "is_active", false);
    else if (pending.action === "offline") await save(product, "offline_days", pending.days);
    else {
      await api("DELETE", `/products/${product.id}`);
      list.reload();
    }
    setPending(null);
  }

  function message(product: Product, pending: NonNullable<Pending>): string {
    if (pending.action === "deactivate") return `Deactivate ${product.name}? Every computer using it locks at its next check.${offlineLimit(product)} No new licences can be issued for it.`;
    if (pending.action === "offline") return `Limit ${product.name} to ${plural(pending.days, "day")} offline? Each computer gets the new limit at its next check online; until then it keeps the one it has.`;
    const { licences, in_use } = pending.usage;
    if (licences === 0) return `Delete ${product.name}?`;
    return `Delete ${product.name} and its ${plural(licences, "licence")}? ${inUseWarning(in_use)}The keys, the licences and their history are deleted. This can't be undone.`;
  }
  return (
    <section className="page">
      <h1>Products</h1>
      <Button variant="primary" size="large" className="page__primary" onClick={() => setAdding(true)} disabledReason={adding ? "Fill in the new row first" : null}>
        New product
      </Button>
      <Toolbar>
        <FilterSelect label="Show" value={active} options={ACTIVE_OPTIONS} onChange={(value) => setTable({ filters: { active: value } })} />
      </Toolbar>
      {created && <TestLicenceNotice product={created} onDismiss={() => setCreated(null)} />}
      <ErrorNotice error={error ?? list.error} />
      <DataTable
        label="Products"
        rows={list.rows}
        columns={columns(save, setActive, (product, action) => void ask(product, action), setApplications, setOfflineDays, (product, price) => save(product, "price", price))}
        rowKey={(product) => product.id}
        loading={list.loading}
        error={list.error}
        firstRow={adding ? <NewProductRow onCreated={(product) => { setAdding(false); setCreated(product); list.reload(); }} onCancel={() => setAdding(false)} /> : null}
        below={(product) =>
          pending?.id === product.id ? (
            <ConfirmStrip
              key={pending.action}
              message={message(product, pending)}
              confirmLabel={CONFIRM_LABELS[pending.action]}
              confirmWord={pending.action === "delete" && pending.usage.licences > 0 ? product.code : undefined}
              onConfirm={() => confirm(product, pending)}
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
