// The licences: search by key, customer or ID, filter, select, act in bulk, and issue new ones
// (the page's primary action).

import { useEffect, useState } from "react";
import { LICENCE_STATE_OPTIONS } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { DataTable } from "../shared/components/DataTable";
import { ColumnPicker, FilterSelect, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useSelection, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { navigate, useLocation } from "../shared/router";
import type { Licence } from "../shared/types";
import { BulkBar } from "./BulkBar";
import { licenceColumns } from "./columns";
import { IssuePanel } from "./IssuePanel";
import { listPath, type LicenceFilters } from "./licenceApi";
import { useProducts } from "./useProducts";

const EXPIRY_OPTIONS = [
  { value: "", label: "Any expiry" },
  { value: "permanent", label: "Permanent" },
  { value: "expiring", label: "Expiring within 30 days" },
  { value: "expired", label: "Expired" },
];
const ORDER_OPTIONS = [
  { value: "newest", label: "Newest first" },
  { value: "expiry", label: "Expiring soonest" },
];
const LINKED_FILTERS = ["product", "customer", "state"] as const;

export function LicencesPage() {
  useTitle("Licences");
  const { search: params } = useLocation();
  const [table, setTable] = useTablePreferences("licences");
  const [search, setSearch] = useState(table.search ?? "");
  const [issuing, setIssuing] = useState(false);
  const { products, error: productsError } = useProducts();
  const filters: LicenceFilters = { ...table.filters, q: useDebounced(search) };
  const order = table.order ?? "newest";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const path = listPath(filters, order);
  const list = usePagedList<Licence>(path, pageSize);
  const selection = useSelection(path);

  // A link from another page (`?customer=…`) sets the filters, then the address is tidied up.
  useEffect(() => {
    const linked = Object.fromEntries(LINKED_FILTERS.flatMap((name) => (params.get(name) ? [[name, params.get(name) as string]] : [])));
    if (Object.keys(linked).length === 0) return;
    setTable({ filters: linked });
    setSearch("");
    navigate("/licences", { replace: true });
  }, [params, setTable]);

  useEffect(() => setTable({ search: filters.q || undefined }), [filters.q, setTable]);

  function setFilter(name: string, value: string) {
    setTable({ filters: { ...table.filters, [name]: value } });
  }

  const replace = (licence: Licence) => list.replace(licence, (existing) => existing.id === licence.id);
  const columns = licenceColumns(replace);
  const customerFilter = table.filters?.customer;

  return (
    <section className="page">
      <h1>Licences</h1>
      {issuing ? (
        <IssuePanel products={products} onClose={() => setIssuing(false)} onIssued={list.reload} />
      ) : (
        <Button variant="primary" size="large" className="page__primary" onClick={() => setIssuing(true)}>
          Issue licences
        </Button>
      )}
      <ErrorNotice error={productsError} />
      <Toolbar>
        <SearchBox label="Search by key, customer or licence ID" value={search} onChange={setSearch} />
        <FilterSelect
          label="Product"
          value={table.filters?.product ?? ""}
          onChange={(value) => setFilter("product", value)}
          options={[{ value: "", label: "Any product" }, ...products.map((product) => ({ value: product.id, label: product.name }))]}
        />
        <FilterSelect label="State" value={table.filters?.state ?? ""} options={LICENCE_STATE_OPTIONS} onChange={(value) => setFilter("state", value)} />
        <FilterSelect label="Expiry" value={table.filters?.expiry ?? ""} options={EXPIRY_OPTIONS} onChange={(value) => setFilter("expiry", value)} />
        <FilterSelect label="Order" value={order} options={ORDER_OPTIONS} onChange={(value) => setTable({ order: value })} />
        <ColumnPicker columns={columns} visible={table.columns} onChange={(columns) => setTable({ columns })} />
      </Toolbar>
      {customerFilter && (
        <p className="filter-chip">
          Showing one customer's licences.{" "}
          <button type="button" className="link-button" onClick={() => setFilter("customer", "")}>
            Show every customer
          </button>
        </p>
      )}
      <BulkBar selection={selection} filters={filters} onDone={list.reload} />
      <ErrorNotice error={list.error} />
      <DataTable
        label="Licences"
        rows={list.rows}
        columns={columns}
        visible={table.columns}
        rowKey={(licence) => licence.id}
        selection={selection}
        hasMore={Boolean(list.next) || list.hasPrevious}
        loading={list.loading}
        empty={search || Object.values(table.filters ?? {}).some(Boolean) ? "No licence matches." : "No licences yet: issue some above."}
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
