// The licences: search by key, customer or ID, filter (each filter can also exclude, and a hide
// box removes what it matches), select, act in bulk, and issue new ones (the page's primary action).

import { useCallback, useEffect, useState } from "react";
import { LICENCE_STATE_OPTIONS } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { DataTable } from "../shared/components/DataTable";
import { ColumnPicker, FilterSelect, onlyOrExcept, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useSelection, useTitle } from "../shared/hooks";
import { useTablePreferences, type IssueDraft } from "../shared/preferences";
import { navigate, useLocation } from "../shared/router";
import type { Licence } from "../shared/types";
import { BulkBar } from "./BulkBar";
import { licenceColumns } from "./columns";
import { IssuePanel } from "./IssuePanel";
import { listPath, type LicenceFilters } from "./licenceApi";
import { useDropDeletedProducts, useProducts } from "./useProducts";

const PRODUCT_FILTERS = ["product", "not_product"] as const;

const EXPIRY_OPTIONS = [
  { value: "permanent", label: "Permanent" },
  { value: "expiring", label: "Expiring within 30 days" },
  { value: "expired", label: "Expired" },
];
const ORDER_OPTIONS = [
  { value: "newest", label: "Newest first" },
  { value: "expiry", label: "Expiring soonest" },
];
const LINKED_FILTERS = ["product", "customer", "not_customer", "state", "not_test"] as const;

const exceptLowercase = (option: { label: string }) => `Except ${option.label.toLowerCase()}`;
const STATE = onlyOrExcept("state", "Any state", LICENCE_STATE_OPTIONS.slice(1), exceptLowercase);
const EXPIRY = onlyOrExcept("expiry", "Any expiry", EXPIRY_OPTIONS, exceptLowercase);
const KIND = onlyOrExcept("test", "Any licence", [{ value: "true", label: "Test licences" }], exceptLowercase);

export function LicencesPage() {
  useTitle("Licences");
  const { search: params } = useLocation();
  const [table, setTable] = useTablePreferences("licences");
  const [search, setSearch] = useState(table.search ?? "");
  const [hide, setHide] = useState(table.filters?.not_q ?? "");
  const issuing = table.issue?.open ?? false;
  // the keys just issued stay on screen this visit, though the panel is remembered as closed
  const [showingKeys, setShowingKeys] = useState(false);
  const issueDraft = table.issue;
  const setIssue = useCallback(
    (change: Partial<IssueDraft>) => setTable({ issue: { open: issueDraft?.open ?? false, ...issueDraft, ...change } }),
    [issueDraft, setTable],
  );
  const [notice, setNotice] = useState<string | null>(null);
  const { products, loaded: productsLoaded, error: productsError } = useProducts();
  const settledHide = useDebounced(hide);
  const filters: LicenceFilters = { ...table.filters, q: useDebounced(search), not_q: settledHide };
  const order = table.order ?? "newest";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const path = listPath(filters, order);
  const list = usePagedList<Licence>(path, pageSize);
  const selection = useSelection(path);

  // A link from another page (`?customer=…`) sets the filters, and a deleted licence's page leaves
  // `?deleted=1` for the notice; then the address is tidied up.
  useEffect(() => {
    const linked = Object.fromEntries(LINKED_FILTERS.flatMap((name) => (params.get(name) ? [[name, params.get(name) as string]] : [])));
    const deleted = params.get("deleted") !== null;
    if (Object.keys(linked).length === 0 && !deleted) return;
    if (deleted) setNotice("Licence deleted.");
    if (Object.keys(linked).length > 0) {
      setTable({ filters: linked });
      setSearch("");
      setHide("");
    }
    navigate("/licences", { replace: true });
  }, [params, setTable]);

  useEffect(() => setTable({ search: filters.q || undefined }), [filters.q, setTable]);

  useDropDeletedProducts(table.filters, PRODUCT_FILTERS, { products, loaded: productsLoaded }, setFilters);

  // The hide text is a filter like the others, saved with them once typing settles.
  useEffect(() => {
    if ((table.filters?.not_q ?? "") !== settledHide) setFilters({ not_q: settledHide });
  }, [settledHide]);

  function setFilters(changes: Record<string, string>) {
    setTable({ filters: { ...table.filters, ...changes } });
  }
  const setFilter = (name: string, value: string) => setFilters({ [name]: value });
  const productFilter = onlyOrExcept(
    "product",
    "Any product",
    products.map((product) => ({ value: product.id, label: product.name })),
    (option) => `Except ${option.label}`,
  );

  const replace = (licence: Licence) => list.replace(licence, (existing) => existing.id === licence.id);
  const columns = licenceColumns(replace);
  const customerFilter = table.filters?.customer;
  const hiddenCustomer = table.filters?.not_customer;
  const shown = table.filters ?? {};

  return (
    <section className="page">
      <h1>Licences</h1>
      {issuing || showingKeys ? (
        <IssuePanel
          products={products}
          productsLoaded={productsLoaded}
          productsError={productsError}
          draft={table.issue ?? { open: true }}
          onDraft={setIssue}
          onClose={() => {
            setShowingKeys(false);
            setIssue({ open: false });
          }}
          onIssued={() => {
            setShowingKeys(true);
            setIssue({ open: false });
            list.reload();
          }}
          onMore={() => {
            setShowingKeys(false);
            setIssue({ open: true });
          }}
        />
      ) : (
        <Button variant="primary" size="large" className="page__primary" onClick={() => setIssue({ open: true })}>
          Issue licences
        </Button>
      )}
      {notice && <Notice tone="success">{notice}</Notice>}
      <ErrorNotice error={productsError} />
      <Toolbar>
        <SearchBox label="Search by key, customer or licence ID" value={search} onChange={setSearch} />
        <SearchBox label="Hide key, customer or licence ID" value={hide} onChange={setHide} />
        <FilterSelect label="Product" value={productFilter.read(shown)} options={productFilter.options} onChange={(value) => setFilters(productFilter.write(value))} />
        <FilterSelect label="State" value={STATE.read(shown)} options={STATE.options} onChange={(value) => setFilters(STATE.write(value))} />
        <FilterSelect label="Expiry" value={EXPIRY.read(shown)} options={EXPIRY.options} onChange={(value) => setFilters(EXPIRY.write(value))} />
        <FilterSelect label="Kind" value={KIND.read(shown)} options={KIND.options} onChange={(value) => setFilters(KIND.write(value))} />
        <FilterSelect label="Order" value={order} options={ORDER_OPTIONS} onChange={(value) => setTable({ order: value })} />
        <ColumnPicker columns={columns} visible={table.columns} onChange={(columns) => setTable({ columns })} />
      </Toolbar>
      {customerFilter && (
        <p className="filter-chip">
          Showing one customer's licences.{" "}
          <button type="button" className="link-button" onClick={() => setFilter("customer", "")}>
            Show every customer
          </button>{" "}
          <button type="button" className="link-button" onClick={() => setFilters({ customer: "", not_customer: customerFilter })}>
            Hide this customer instead
          </button>
        </p>
      )}
      {hiddenCustomer && (
        <p className="filter-chip">
          Hiding one customer's licences.{" "}
          <button type="button" className="link-button" onClick={() => setFilter("not_customer", "")}>
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
        error={list.error}
        empty={search || hide || Object.values(shown).some(Boolean) ? "No licence matches." : "No licences yet: issue some above."}
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
