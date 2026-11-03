// One licence: every field, its key (Reveal/Copy), Copy message, Reset (only while in use),
// Revoke (unless revoked), and every computer it was ever activated on.

import { useEffect, useState } from "react";
import { api, get } from "../shared/api";
import { LicenceStateBadge } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { CopyButton, KeyField } from "../shared/components/Copy";
import { ErrorNotice, Loading } from "../shared/components/Notice";
import { formatDateTime } from "../shared/format";
import { useTitle } from "../shared/hooks";
import { Link } from "../shared/router";
import { expanded, type Licence } from "../shared/types";
import { ActivationHistory } from "./ActivationHistory";
import { ExpiryField, NotesField } from "./columns";
import { LICENCE_EXPAND } from "./licenceApi";

type Action = "reset" | "revoke" | null;

function resetReason(licence: Licence): string | null {
  if (licence.state === "revoked") return "A revoked licence can't be reset";
  return licence.state === "redeemed" ? null : "This licence isn't in use on any computer";
}

export function LicenceDetailPage({ id }: { id: string }) {
  useTitle("Licence");
  const [licence, setLicence] = useState<Licence | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [action, setAction] = useState<Action>(null);
  const [history, setHistory] = useState(0);

  useEffect(() => {
    get<Licence>(`/licences/${id}?expand=${LICENCE_EXPAND}`).then(setLicence, setError);
  }, [id]);

  if (!licence) return error ? <ErrorNotice error={error} /> : <Loading />;
  const product = expanded(licence.product);
  const customer = expanded(licence.customer);
  const revoked = licence.state === "revoked";

  async function act(kind: "reset" | "revoke", reason: string | null) {
    const body = kind === "revoke" ? { reason: reason ?? undefined } : undefined;
    setLicence(await api<Licence>("POST", `/licences/${id}/${kind}?expand=${LICENCE_EXPAND}`, body));
    setAction(null);
    setHistory((value) => value + 1);
  }

  return (
    <section className="page">
      <p className="breadcrumb">
        <Link to="/licences">Licences</Link>
      </p>
      <h1>
        Licence <LicenceStateBadge state={licence.state} />
      </h1>
      <dl className="details">
        <dt>Key</dt>
        <dd><KeyField licenceId={licence.id} revoked={revoked} /></dd>
        <dt>Product</dt>
        <dd>{product?.name ?? "—"}</dd>
        <dt>Customer</dt>
        <dd>{customer ? `${customer.name}${customer.email ? ` · ${customer.email}` : ""}` : <span className="muted">No customer</span>}</dd>
        <dt>Issued</dt>
        <dd>{formatDateTime(licence.created_at)}</dd>
        <dt>Expires</dt>
        <dd><ExpiryField licence={licence} onChange={setLicence} /></dd>
        <dt>Notes</dt>
        <dd><NotesField licence={licence} onChange={setLicence} /></dd>
        {revoked && (
          <>
            <dt>Revoked</dt>
            <dd>{formatDateTime(licence.revoked_at)}{licence.revocation_reason ? ` · ${licence.revocation_reason}` : ""}</dd>
          </>
        )}
        <dt>Licence ID</dt>
        <dd><code className="licence-id">{licence.id}</code></dd>
      </dl>
      <div className="button-row">
        <CopyButton
          size="normal"
          label="Copy message"
          text={() => api<{ message: string }>("POST", `/licences/${id}/message`, {}).then((answer) => answer.message)}
        />
        <Button onClick={() => setAction("reset")} disabledReason={resetReason(licence)}>Reset</Button>
        <Button variant="danger" onClick={() => setAction("revoke")} disabledReason={revoked ? "This licence is already revoked" : null}>
          Revoke
        </Button>
      </div>
      {action === "reset" && (
        <ConfirmStrip
          message="Reset this licence? The computer using it locks at its next check, and the same key can then be activated on another computer."
          confirmLabel="Reset"
          onConfirm={() => act("reset", null)}
          onCancel={() => setAction(null)}
        />
      )}
      {action === "revoke" && (
        <ConfirmStrip
          message="Revoke this licence? Its key is deleted and can never be used again, and the computer using it locks at its next check. This can't be undone."
          confirmLabel="Revoke"
          withReason
          onConfirm={(reason) => act("revoke", reason)}
          onCancel={() => setAction(null)}
        />
      )}
      <h2>Computers</h2>
      <ActivationHistory licenceId={licence.id} key={history} />
    </section>
  );
}
