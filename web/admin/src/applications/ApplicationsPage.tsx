// Licence applications, sent from an application or the web form: who, for what, their message,
// and where each stands. Waiting ones are approved (inline, issuing the licence) or rejected with
// an optional reason the applicant is shown; any can be deleted. Bulk: reject or delete a
// selection, or everything matching the filter.

import { useState } from "react";
import { api } from "../shared/api";
import { Badge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { DataTable, type Column } from "../shared/components/DataTable";
import { ColumnPicker, FilterSelect, Pager, SearchBox, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import { formatDateTime } from "../shared/format";
import { DEFAULT_PAGE_SIZE, useDebounced, usePagedList, useSelection, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import { expanded, idOf } from "../shared/types";
import { useDropDeletedProducts, useProducts } from "../licences/useProducts";
import { listPath, matchingIds, type Application, type ApplicationState } from "./applicationApi";
import { ApproveForm } from "./ApproveForm";

const PRODUCT_FILTERS = ["product"] as const;
const STATE_OPTIONS = [
  { value: "pending", label: "Waiting" },
  { value: "approved", label: "Approved" },
  { value: "rejected", label: "Rejected" },
  { value: "all", label: "All" },
];
const STATES: Record<ApplicationState, { label: string; tone: string }> = {
  pending: { label: "Waiting", tone: "neutral" },
  approved: { label: "Approved", tone: "good" },
  rejected: { label: "Rejected", tone: "bad" },
};

type Open = { id: string; action: "approve" | "reject" | "delete" } | null;
type Bulk = { action: "reject" | "delete"; ids: string[] } | null;

function Outcome({ application }: { application: Application }) {
  if (application.state === "rejected") {
    return application.rejection_reason ? <span className="clamp">{application.rejection_reason}</span> : <span className="muted">No reason given</span>;
  }
  if (application.state === "pending") return <span className="muted">—</span>;
  const licence = idOf(application.licence);
  if (!licence) return <span className="muted">Licence deleted</span>;
  return (
    <span className="stacked">
      <Link to={`/licences/${licence}`}>Open licence</Link>
      <span className="muted">{application.key_collected_at ? `Key collected ${formatDateTime(application.key_collected_at)}` : "Key not collected yet"}</span>
    </span>
  );
}

function columns(open: (id: string, action: "approve" | "reject" | "delete") => void): Column<Application>[] {
  return [
    { key: "received", className: "table__nowrap", label: "Received", render: (application) => formatDateTime(application.created_at) },
    {
      key: "applicant",
      label: "Applicant",
      render: (application) => (
        <span className="stacked">
          <span>{application.name}</span>
          <span className="muted">{application.email}</span>
        </span>
      ),
    },
    { key: "product", label: "Product", optional: true, render: (application) => expanded(application.product)?.name ?? "—" },
    { key: "message", label: "Message", optional: true, render: (application) => (application.message ? <span className="clamp">{application.message}</span> : <span className="muted">—</span>) },
    { key: "source", label: "From", optional: true, render: (application) => (application.source === "web" ? "Web form" : "Application") },
    { key: "state", label: "State", render: (application) => <Badge tone={STATES[application.state].tone}>{STATES[application.state].label}</Badge> },
    { key: "outcome", label: "Outcome", optional: true, render: (application) => <Outcome application={application} /> },
    {
      key: "actions",
      label: "",
      className: "table__action",
      render: (application) => {
        const decided = application.state !== "pending" ? "This application has already been decided" : null;
        return (
          <span className="button-row">
            <Button size="small" variant="quiet" onClick={() => open(application.id, "approve")} disabledReason={decided}>
              Approve
            </Button>
            <Button size="small" variant="quiet" onClick={() => open(application.id, "reject")} disabledReason={decided}>
              Reject
            </Button>
            <Button size="small" variant="quiet" onClick={() => open(application.id, "delete")}>
              Delete
            </Button>
          </span>
        );
      },
    },
  ];
}

export function ApplicationsPage() {
  useTitle("Applications");
  const [table, setTable] = useTablePreferences("applications");
  const [search, setSearch] = useState(table.search ?? "");
  const [opened, setOpened] = useState<Open>(null);
  const [bulk, setBulk] = useState<Bulk>(null);
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const { products, loaded: productsLoaded, error: productsError } = useProducts();
  const q = useDebounced(search);
  const state = table.filters?.state ?? "pending";
  const product = table.filters?.product ?? "";
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const filters = { state, product, q };
  const path = listPath(filters);
  const list = usePagedList<Application>(path, pageSize);
  const selection = useSelection(path);
  const shown = columns((id, action) => {
    setDone(null);
    setOpened({ id, action });
  });

  function setFilter(name: string, value: string) {
    setTable({ filters: { ...table.filters, state, [name]: value } });
  }
  useDropDeletedProducts(table.filters, PRODUCT_FILTERS, { products, loaded: productsLoaded }, (changes) =>
    setTable({ filters: { ...table.filters, state, ...changes } }),
  );

  async function reject(id: string, reason: string | null) {
    await api("POST", `/applications/${id}/reject`, { reason });
    setOpened(null);
    list.reload();
  }
  async function remove(id: string) {
    await api("DELETE", `/applications/${id}`);
    setOpened(null);
    list.reload();
  }
  async function chooseBulk(action: "reject" | "delete") {
    setError(null);
    setDone(null);
    try {
      const ids = selection.allMatching
        ? (await matchingIds(filters)).filter((id) => !selection.excluded.has(id))
        : [...selection.ids];
      setBulk({ action, ids });
    } catch (failure) {
      setError(failure);
    }
  }
  async function runBulk(reason: string | null) {
    if (!bulk) return;
    const body = bulk.action === "reject" ? { ids: bulk.ids, reason } : { ids: bulk.ids };
    await api("POST", `/applications/bulk/${bulk.action}`, body);
    setDone(`${plural(bulk.ids.length, "application")} ${bulk.action === "reject" ? "rejected" : "deleted"}.`);
    setBulk(null);
    selection.clear();
    list.reload();
  }
  const chosen = selection.allMatching ? "every application that matches" : plural(selection.ids.size, "application");
  const anyChosen = selection.allMatching || selection.ids.size > 0;

  return (
    <section className="page">
      <h1>Applications</h1>
      <p className="muted">
        People apply from inside an application, or on the web form at <a href="/apply">/apply</a>, for products that
        take applications (on the Products page).
      </p>
      <Toolbar>
        <SearchBox label="Search by name or email" value={search} onChange={(value) => { setSearch(value); setTable({ search: value || undefined }); }} />
        <FilterSelect label="State" value={state} options={STATE_OPTIONS} onChange={(value) => setFilter("state", value)} />
        <FilterSelect
          label="Product"
          value={product}
          onChange={(value) => setFilter("product", value)}
          options={[{ value: "", label: "Any product" }, ...products.map((option) => ({ value: option.id, label: option.name }))]}
        />
        <ColumnPicker columns={shown} visible={table.columns} onChange={(visible) => setTable({ columns: visible })} />
      </Toolbar>
      {anyChosen && (
        <div className="bulk-bar" role="group" aria-label="Selected applications">
          <span className="bulk-bar__count">{chosen} selected</span>
          <Button size="small" onClick={() => void chooseBulk("reject")}>Reject</Button>
          <Button size="small" onClick={() => void chooseBulk("delete")}>Delete</Button>
          <Button size="small" variant="quiet" onClick={selection.clear}>Clear selection</Button>
        </div>
      )}
      {bulk && (
        <ConfirmStrip
          key={bulk.action}
          message={
            bulk.action === "reject"
              ? `Reject ${plural(bulk.ids.length, "application")}? Each applicant is shown the reason, if you give one. Only waiting applications can be rejected.`
              : `Delete ${plural(bulk.ids.length, "application")}? Their names, emails and messages are deleted; licences issued for them stay.`
          }
          confirmLabel={bulk.action === "reject" ? "Reject" : "Delete"}
          withReason={bulk.action === "reject"}
          onConfirm={runBulk}
          onCancel={() => setBulk(null)}
        />
      )}
      {done && <Notice tone="success">{done}</Notice>}
      <ErrorNotice error={error ?? productsError ?? list.error} item="application" />
      <DataTable
        label="Applications"
        rows={list.rows}
        columns={shown}
        visible={table.columns}
        rowKey={(application) => application.id}
        selection={selection}
        hasMore={Boolean(list.next) || list.hasPrevious}
        loading={list.loading}
        error={list.error}
        below={(application) => {
          if (opened?.id !== application.id) return null;
          if (opened.action === "approve") {
            return (
              <ApproveForm
                application={application}
                onApproved={(approved) => {
                  list.replace(approved, (existing) => existing.id === approved.id);
                  setOpened(null);
                  setDone(`Approved: ${application.name}'s licence was issued.`);
                }}
                onCancel={() => setOpened(null)}
              />
            );
          }
          return opened.action === "reject" ? (
            <ConfirmStrip
              message={`Reject ${application.name}'s application? They are shown the reason, if you give one.`}
              confirmLabel="Reject"
              withReason
              onConfirm={(reason) => reject(application.id, reason)}
              onCancel={() => setOpened(null)}
            />
          ) : (
            <ConfirmStrip
              message={`Delete ${application.name}'s application? Their name, email and message are deleted${application.state === "approved" ? "; the licence issued for it stays" : ""}.`}
              confirmLabel="Delete"
              onConfirm={() => remove(application.id)}
              onCancel={() => setOpened(null)}
            />
          );
        }}
        empty={state === "pending" ? "No application is waiting." : "No application matches."}
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
