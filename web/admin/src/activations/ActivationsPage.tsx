// The computers using licences: product, licence, customer, device, versions, dates and state.
// Revoking one here is the same as resetting its licence.

import { useState } from "react";
import { api, query } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { DataTable, type Column } from "../shared/components/DataTable";
import { ColumnPicker, FilterSelect, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { formatDateTime } from "../shared/format";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import { expanded, idOf, type Activation } from "../shared/types";
import { useProducts } from "../licences/useProducts";
import { activationState } from "./activationState";

const EXPAND = "licence.product,licence.customer";
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
    { key: "licence", label: "Licence", optional: true, render: (activation) => <Link to={`/licences/${idOf(activation.licence)}`}>Open</Link> },
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

export function ActivationsPage() {
  useTitle("Activations");
  const [table, setTable] = useTablePreferences("activations");
  const [search, setSearch] = useState(table.search ?? "");
  const [revoking, setRevoking] = useState<string | null>(null);
  const { products, error: productsError } = useProducts();
  const q = useDebounced(search);
  const state = table.filters?.state ?? "active";
  const product = table.filters?.product ?? "";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const list = usePagedList<Activation>(`/activations${query({ state: state === "all" ? undefined : state, product, q, expand: EXPAND })}`, pageSize);
  const shown = columns(setRevoking);

  function setFilter(name: string, value: string) {
    setTable({ filters: { ...table.filters, state, [name]: value } });
  }
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
        below={(activation) =>
          revoking === activation.id ? (
            <ConfirmStrip
              message={`Revoke ${activation.device_name}? It is the same as resetting its licence: this computer locks at its next check, and the key can then be activated on another one.`}
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
