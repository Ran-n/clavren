// The computers using licences: product, licence, customer, device, versions, dates and state.
// Revoking one here ends that computer's activation: the same as resetting its licence, except
// that a test licence's other computers go on.

import { useState } from "react";
import { api, query } from "../shared/api";
import { TestBadge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { DataTable, type Column } from "../shared/components/DataTable";
import { ColumnPicker, FilterSelect, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { offlineLimit } from "../shared/errors";
import { formatDateTime } from "../shared/format";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import { expanded, idOf, type Activation } from "../shared/types";
import { useDropDeletedProducts, useProducts } from "../licences/useProducts";
import { activationState } from "./activationState";

const EXPAND = "licence.product,licence.customer";
const PRODUCT_FILTERS = ["product"] as const;
const KIND_OPTIONS = [
  { value: "", label: "Any licence" },
  { value: "true", label: "Test licences" },
  { value: "false", label: "Except test licences" },
];
const STATE_OPTIONS = [
  { value: "active", label: "In use" },
  { value: "revoked", label: "Ended" },
  { value: "all", label: "All" },
];

function columns(onRevoke: (id: string) => void): Column<Activation>[] {
  const licence = (activation: Activation) => expanded(activation.licence);
  return [
    { key: "device", label: "Computer", render: (activation) => activation.device_name },
    { key: "product", label: "Product", optional: true, render: (activation) => expanded(licence(activation)?.product)?.name ?? "—" },
    { key: "customer", label: "Customer", optional: true, render: (activation) => expanded(licence(activation)?.customer)?.name ?? <span className="muted">No customer</span> },
    { key: "licence", label: "Licence", optional: true, render: (activation) => (
        <span className="badges">
          <Link to={`/licences/${idOf(activation.licence)}`}>Open</Link>
          {licence(activation)?.is_test && <TestBadge />}
        </span>
      ),
    },
    { key: "platform", label: "Platform", optional: true, render: (activation) => activation.platform },
    { key: "version", label: "App version", optional: true, render: (activation) => activation.app_version },
    { key: "activated", className: "table__nowrap", label: "Activated", optional: true, render: (activation) => formatDateTime(activation.activated_at) },
    { key: "last_check", className: "table__nowrap", label: "Last check", optional: true, render: (activation) => formatDateTime(activation.last_checked_at) },
    { key: "state", label: "State", render: activationState },
    {
      key: "actions",
      label: "",
      className: "table__action",
      render: (activation) => (
        <Button size="small" variant="quiet" onClick={() => onRevoke(activation.id)} disabledReason={activation.revoked_at ? "This activation has already ended" : null}>
          Revoke
        </Button>
      ),
    },
  ];
}

function revokeMessage(activation: Activation): string {
  const licence = expanded(activation.licence);
  const limit = offlineLimit(expanded(licence?.product));
  if (licence?.is_test) {
    return `Revoke ${activation.device_name}? This computer locks at its next check.${limit} The test licence's other computers keep working.`;
  }
  return `Revoke ${activation.device_name}? It is the same as resetting its licence: this computer locks at its next check.${limit} The key can then be activated on another one.`;
}

export function ActivationsPage() {
  useTitle("Activations");
  const [table, setTable] = useTablePreferences("activations");
  const [search, setSearch] = useState(table.search ?? "");
  const [revoking, setRevoking] = useState<string | null>(null);
  const { products, loaded: productsLoaded, error: productsError } = useProducts();
  const q = useDebounced(search);
  const state = table.filters?.state ?? "active";
  const product = table.filters?.product ?? "";
  const kind = table.filters?.test ?? "";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const list = usePagedList<Activation>(`/activations${query({ state: state === "all" ? undefined : state, product, test: kind, q, expand: EXPAND })}`, pageSize);
  const shown = columns(setRevoking);

  function setFilter(name: string, value: string) {
    setTable({ filters: { ...table.filters, state, [name]: value } });
  }
  useDropDeletedProducts(table.filters, PRODUCT_FILTERS, { products, loaded: productsLoaded }, (changes) =>
    setTable({ filters: { ...table.filters, state, ...changes } }),
  );
  async function revoke(id: string) {
    await api("POST", `/activations/${id}/revoke`, {});
    setRevoking(null);
    list.reload();
  }

  return (
    <section className="page">
      <h1>Activations</h1>
      <Toolbar>
        <SearchBox label="Search by computer, platform or version" value={search} onChange={(value) => { setSearch(value); setTable({ search: value || undefined }); }} />
        <FilterSelect label="State" value={state} options={STATE_OPTIONS} onChange={(value) => setFilter("state", value)} />
        <FilterSelect
          label="Product"
          value={product}
          onChange={(value) => setFilter("product", value)}
          options={[{ value: "", label: "Any product" }, ...products.map((option) => ({ value: option.id, label: option.name }))]}
        />
        <FilterSelect label="Kind" value={kind} options={KIND_OPTIONS} onChange={(value) => setFilter("test", value)} />
        <ColumnPicker columns={shown} visible={table.columns} onChange={(visible) => setTable({ columns: visible })} />
      </Toolbar>
      <ErrorNotice error={productsError ?? list.error} />
      <DataTable
        label="Activations"
        rows={list.rows}
        columns={shown}
        visible={table.columns}
        rowKey={(activation) => activation.id}
        loading={list.loading}
        error={list.error}
        below={(activation) =>
          revoking === activation.id ? (
            <ConfirmStrip
              message={revokeMessage(activation)}
              confirmLabel="Revoke"
              onConfirm={() => revoke(activation.id)}
              onCancel={() => setRevoking(null)}
            />
          ) : null
        }
        empty="No computer matches."
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
