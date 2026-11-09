// The audit log: who did what, when, from where. Filters by action, admin, record and date.

import { query } from "../shared/api";
import { DataTable, type Column } from "../shared/components/DataTable";
import { FilterSelect, Pager, Toolbar } from "../shared/components/ListControls";
import { ErrorNotice } from "../shared/components/Notice";
import { formatDateTime } from "../shared/format";
import { DEFAULT_PAGE_SIZE, usePagedList, useTitle } from "../shared/hooks";
import { useTablePreferences } from "../shared/preferences";
import { Link } from "../shared/router";
import { expanded, type AuditEntry } from "../shared/types";
import { ACTION_FILTERS, describeEntry } from "./actions";

/** A `2027-03-01` date input's day, at its start in the browser's time zone; none if it isn't one. */
function day(value: string | undefined): Date | undefined {
  if (!value) return undefined;
  const start = new Date(`${value}T00:00:00`);
  // a remembered filter may hold anything: an unreadable date is no filter, not a blank page
  return Number.isNaN(start.getTime()) ? undefined : start;
}

/** The first day included and the first day not: whole days in the browser's time zone. */
export function dayStart(value: string | undefined): string | undefined {
  return day(value)?.toISOString();
}
export function dayAfter(value: string | undefined): string | undefined {
  const after = day(value);
  if (!after) return undefined;
  after.setDate(after.getDate() + 1);
  return after.toISOString();
}

/** Without an admin, an entry comes from the application API or from `clavren-admin`. */
function byApplication(action: string): boolean {
  return action.startsWith("activation.") || action.startsWith("check.");
}

function Entity({ entry, onPick }: { entry: AuditEntry; onPick: (id: string) => void }) {
  if (!entry.entity_id) return <span className="muted">{entry.entity_type}</span>;
  // The short ID in a fixed-width font, so the column is as wide whatever the ID.
  const label = (
    <>
      {entry.entity_type} <code>{entry.entity_id.slice(-6)}</code>
    </>
  );
  return (
    <span className="button-row">
      {entry.entity_type === "licence" ? <Link to={`/licences/${entry.entity_id}`}>{label}</Link> : <span>{label}</span>}
      <button type="button" className="link-button" onClick={() => onPick(entry.entity_id as string)}>
        Only this
      </button>
    </span>
  );
}

function columns(onAdmin: (id: string) => void, onEntity: (id: string) => void): Column<AuditEntry>[] {
  return [
    { key: "when", className: "table__nowrap", label: "When", render: (entry) => formatDateTime(entry.created_at) },
    {
      key: "who",
      label: "Who",
      render: (entry) => {
        const admin = expanded(entry.admin_user);
        if (!admin) return <span className="muted">{byApplication(entry.action) ? "The application" : "clavren-admin, on the server"}</span>;
        return <button type="button" className="link-button" onClick={() => onAdmin(admin.id)}>{admin.username}</button>;
      },
    },
    { key: "what", label: "What", render: (entry) => describeEntry(entry.action, entry.metadata) },
    { key: "record", label: "Record", render: (entry) => <Entity entry={entry} onPick={onEntity} /> },
    { key: "ip", label: "From", render: (entry) => entry.ip_address ?? "—" },
  ];
}

export function AuditPage() {
  useTitle("Audit log");
  const [table, setTable] = useTablePreferences("audit");
  const filters = table.filters ?? {};
  const pageSize = table.page_size ?? DEFAULT_PAGE_SIZE;
  const path = `/audit${query({ action: filters.action, admin: filters.admin, entity: filters.entity, from: dayStart(filters.from), to: dayAfter(filters.to), expand: "admin_user" })}`;
  const list = usePagedList<AuditEntry>(path, pageSize);

  function setFilter(name: string, value: string) {
    setTable({ filters: { ...filters, [name]: value } });
  }
  const narrowed = filters.admin || filters.entity;

  return (
    <section className="page">
      <h1>Audit log</h1>
      <Toolbar>
        <FilterSelect label="Action" value={filters.action ?? ""} options={ACTION_FILTERS} onChange={(value) => setFilter("action", value)} />
        <label className="filter">
          <span className="filter__label">From</span>
          <input type="date" value={filters.from ?? ""} onChange={(event) => setFilter("from", event.target.value)} />
        </label>
        <label className="filter">
          <span className="filter__label">To</span>
          <input type="date" value={filters.to ?? ""} onChange={(event) => setFilter("to", event.target.value)} />
        </label>
      </Toolbar>
      {narrowed && (
        <p className="filter-chip">
          Showing {filters.admin ? "one admin" : "one record"}.{" "}
          <button type="button" className="link-button" onClick={() => setTable({ filters: { ...filters, admin: "", entity: "" } })}>
            Show everything
          </button>
        </p>
      )}
      <ErrorNotice error={list.error} />
      <DataTable
        label="Audit log"
        rows={list.rows}
        columns={columns((id) => setFilter("admin", id), (id) => setFilter("entity", id))}
        rowKey={(entry) => entry.id}
        loading={list.loading}
        error={list.error}
        empty="Nothing recorded matches."
      />
      <Pager list={list} pageSize={pageSize} onPageSize={(size) => setTable({ page_size: size })} />
    </section>
  );
}
