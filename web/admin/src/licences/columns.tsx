// The licence table's columns. Customer, expiry and notes are edited in place (and a revoked
// licence's reason, on its page); the key stays hidden until Reveal.

import { LicenceStateBadge } from "../shared/components/Badge";
import { KeyField } from "../shared/components/Copy";
import type { Column } from "../shared/components/DataTable";
import { InlineText } from "../shared/components/InlineText";
import { formatDate, formatDateTime, fromDateInput, toDateInput } from "../shared/format";
import { Link } from "../shared/router";
import { expanded, type Licence } from "../shared/types";
import { CustomerField } from "./CustomerField";
import { updateLicence } from "./licenceApi";

/** The server's limit on a revocation reason. */
const MAX_REASON = 500;

type Replace = (licence: Licence) => void;

export function ExpiryField({ licence, onChange }: { licence: Licence; onChange: Replace }) {
  if (licence.state === "revoked") return <>{formatDate(licence.expires_at)}</>;
  return (
    <InlineText
      label="expiry date"
      type="date"
      value={toDateInput(licence.expires_at)}
      display={formatDate(licence.expires_at)}
      placeholder="Permanent"
      onSave={async (value) => onChange(await updateLicence(licence.id, { expires_at: value ? fromDateInput(value) : null }))}
    />
  );
}

export function NotesField({ licence, onChange }: { licence: Licence; onChange: Replace }) {
  return (
    <InlineText
      label="notes"
      value={licence.internal_notes}
      maxLength={2000}
      placeholder="Add a note"
      onSave={async (value) => onChange(await updateLicence(licence.id, { internal_notes: value }))}
    />
  );
}

export function ReasonField({ licence, onChange }: { licence: Licence; onChange: Replace }) {
  return (
    <InlineText
      label="revocation reason"
      value={licence.revocation_reason}
      maxLength={MAX_REASON}
      placeholder="Add a reason"
      onSave={async (value) => onChange(await updateLicence(licence.id, { revocation_reason: value }))}
    />
  );
}

export function licenceColumns(onChange: Replace): Column<Licence>[] {
  return [
    { key: "key", label: "Key", render: (licence) => <KeyField licenceId={licence.id} revoked={licence.state === "revoked"} /> },
    { key: "product", label: "Product", optional: true, render: (licence) => expanded(licence.product)?.name ?? "—" },
    { key: "customer", label: "Customer", optional: true, render: (licence) => <CustomerField compact licence={licence} onChange={onChange} /> },
    { key: "state", label: "State", render: (licence) => <LicenceStateBadge state={licence.state} /> },
    { key: "issued", className: "table__nowrap", label: "Issued", optional: true, render: (licence) => formatDate(licence.created_at) },
    { key: "redeemed", label: "Activated", optional: true, render: (licence) => formatDateTime(expanded(licence.active_activation)?.activated_at) },
    { key: "last_check", label: "Last check", optional: true, render: (licence) => formatDateTime(expanded(licence.active_activation)?.last_checked_at) },
    { key: "expiry", className: "table__nowrap", label: "Expires", optional: true, render: (licence) => <ExpiryField licence={licence} onChange={onChange} /> },
    { key: "notes", label: "Notes", optional: true, render: (licence) => <NotesField licence={licence} onChange={onChange} /> },
    {
      key: "open",
      label: "",
      className: "table__action",
      render: (licence) => (
        <Link to={`/licences/${licence.id}`} className="button button--quiet button--small">
          Details
        </Link>
      ),
    },
  ];
}
