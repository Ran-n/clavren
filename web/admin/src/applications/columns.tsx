// The licence applications table's columns: when received, who applied, the product, message and
// source, the state badge, the outcome (the licence, or why it was rejected), and the row's actions.

import { Badge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import type { Column } from "../shared/components/DataTable";
import { formatDateTime } from "../shared/format";
import { Link } from "../shared/router";
import { expanded, idOf } from "../shared/types";
import type { Application, ApplicationState } from "./applicationApi";

const STATES: Record<ApplicationState, { label: string; tone: string }> = {
  pending: { label: "Waiting", tone: "neutral" },
  approved: { label: "Approved", tone: "good" },
  rejected: { label: "Rejected", tone: "bad" },
};

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

export function columns(open: (id: string, action: "approve" | "reject" | "delete") => void): Column<Application>[] {
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
