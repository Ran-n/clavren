// One licence: every field (customer, expiry, notes and a revoked licence's reason edited in
// place), its key (Reveal/Copy), Copy message, Reset (only while in use), Revoke (unless revoked),
// Delete (in any state, back to the list), and every computer it was ever activated on.

import { useEffect, useState } from "react";
import { api, get } from "../shared/api";
import { LicenceBadges } from "../shared/components/Badge";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { CopyButton, KeyField } from "../shared/components/Copy";
import { ErrorNotice, Loading } from "../shared/components/Notice";
import { formatDateTime } from "../shared/format";
import { useTitle } from "../shared/hooks";
import { Link, navigate } from "../shared/router";
import { expanded, type Licence } from "../shared/types";
import { ActivationHistory } from "./ActivationHistory";
import { ExpiryField, NotesField, ReasonField } from "./columns";
import { CustomerField } from "./CustomerField";
import { DELETED_NOTICE, LICENCE_EXPAND } from "./licenceApi";

type Action = "reset" | "revoke" | "delete" | null;

/** The computers a licence is on, as the subject of a sentence: a test licence can be on several. */
function computersUsing(licence: Licence): string {
  return licence.is_test ? "Every computer using it" : "The computer using it";
}

/** What deleting a licence in this state ends, in plain words. */
function deleteMessage(licence: Licence): string {
  switch (licence.state) {
    case "redeemed":
      return `Delete this licence? ${computersUsing(licence)} locks at its next check, the key stops working, and the licence disappears with its history. This can't be undone.`;
    case "unused":
      return "Delete this licence? Its key stops working and the licence disappears. This can't be undone.";
    case "revoked":
      return "Delete this licence? It disappears with the computers it was used on. This can't be undone.";
  }
}

function resetMessage(licence: Licence): string {
  return `Reset this licence? ${computersUsing(licence)} locks at its next check, and the same key can then be activated on another computer.`;
}

function revokeMessage(licence: Licence): string {
  const using = computersUsing(licence).toLowerCase();
  return `Revoke this licence? Its key is deleted and can never be used again, and ${using} locks at its next check. This can't be undone.`;
}

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
  const revoked = licence.state === "revoked";

  async function act(kind: "reset" | "revoke", reason: string | null) {
    const body = kind === "revoke" ? { reason: reason ?? undefined } : undefined;
    setLicence(await api<Licence>("POST", `/licences/${id}/${kind}?expand=${LICENCE_EXPAND}`, body));
    setAction(null);
    setHistory((value) => value + 1);
  }

  async function remove() {
    await api("DELETE", `/licences/${id}`);
    navigate(`/licences?${DELETED_NOTICE}`);
  }

  return (
    <section className="page">
      <p className="breadcrumb">
        <Link to="/licences">Licences</Link>
      </p>
      <h1>
        Licence <LicenceBadges state={licence.state} test={licence.is_test} />
      </h1>
      <dl className="details">
        <dt>Key</dt>
        <dd><KeyField licenceId={licence.id} revoked={revoked} /></dd>
        <dt>Product</dt>
        <dd>{product?.name ?? "—"}</dd>
        {licence.is_test && (
          <>
            <dt>Computers</dt>
            <dd>{licence.active_computers === 1 ? "In use on 1 computer" : `In use on ${licence.active_computers} computers`}</dd>
          </>
        )}
        <dt>Customer</dt>
        <dd><CustomerField licence={licence} onChange={setLicence} /></dd>
        <dt>Issued</dt>
        <dd>{formatDateTime(licence.created_at)}</dd>
        <dt>Expires</dt>
        <dd><ExpiryField licence={licence} onChange={setLicence} /></dd>
        <dt>Notes</dt>
        <dd><NotesField licence={licence} onChange={setLicence} /></dd>
        {revoked && (
          <>
            <dt>Revoked</dt>
            <dd className="revoked-line">
              {formatDateTime(licence.revoked_at)} · <ReasonField licence={licence} onChange={setLicence} />
            </dd>
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
        <Button variant="danger" onClick={() => setAction("delete")}>
          Delete
        </Button>
      </div>
      {action === "reset" && (
        <ConfirmStrip
          message={resetMessage(licence)}
          confirmLabel="Reset"
          onConfirm={() => act("reset", null)}
          onCancel={() => setAction(null)}
        />
      )}
      {action === "revoke" && (
        <ConfirmStrip
          message={revokeMessage(licence)}
          confirmLabel="Revoke"
          withReason
          onConfirm={(reason) => act("revoke", reason)}
          onCancel={() => setAction(null)}
        />
      )}
      {action === "delete" && (
        <ConfirmStrip message={deleteMessage(licence)} confirmLabel="Delete" onConfirm={remove} onCancel={() => setAction(null)} />
      )}
      <h2>Computers</h2>
      <ActivationHistory licenceId={licence.id} key={history} />
    </section>
  );
}
