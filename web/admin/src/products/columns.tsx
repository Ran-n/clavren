// The products table's columns: each field edited in place, the price, offline days, state, the
// key page's open or closed switch with a link to copy, and the row's actions (licences, activate,
// deactivate, delete).

import { Badge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import type { Column } from "../shared/components/DataTable";
import { InlineText } from "../shared/components/InlineText";
import { InlinePrice } from "../shared/components/PriceField";
import { Link } from "../shared/router";
import type { Price, Product } from "../shared/types";
import { OfflineDays } from "./OfflineDays";

export type Field = "code" | "name" | "description" | "message_template" | "key_instructions";

type Ask = (product: Product, action: "deactivate" | "delete") => void;

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

export function columns(save: (product: Product, field: Field, value: string | null) => Promise<void>, setActive: (product: Product, active: boolean) => void, ask: Ask, setApplications: (product: Product, open: boolean) => void, setOfflineDays: (product: Product, days: number) => Promise<void>, setPrice: (product: Product, price: Price | null) => Promise<void>, counting: string | null): Column<Product>[] {
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
          <Button size="small" variant="quiet" busy={counting === product.id} onClick={() => ask(product, "delete")}>Delete</Button>
        </span>
      ),
    },
  ];
}
