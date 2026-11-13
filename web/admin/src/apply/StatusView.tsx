// How an application went, from its private link (`/apply/status#<secret>`): waiting (asked
// again every two minutes, and on Check again), approved with its key and Copy, rejected with the
// administrator's reason, or withdrawn.

import { useCallback, useEffect, useState } from "react";
import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import { Loading } from "../shared/components/Notice";
import { formatDate, formatDateTime } from "../shared/format";
import { askStatus, explain, statusLink, type Status } from "./publicApi";

const ASK_AGAIN_MS = 2 * 60 * 1000;

function Outcome({ status }: { status: Status }) {
  switch (status.status) {
    case "pending":
      return (
        <p className="notice notice--info" role="status">
          Your application is waiting for the administrator. This page checks again by itself; you can also come back
          later with the link.
        </p>
      );
    case "approved":
      return (
        <div className="apply-key">
          <p className="notice notice--success" role="status">
            Approved. Type this key into the application when it asks for one.
          </p>
          <p className="apply-key__row">
            <code className="key">{status.key}</code>
            <CopyButton label="Copy key" text={() => Promise.resolve(status.key ?? "")} />
          </p>
          <p className="field__help">
            {status.licence_expires_at ? `The licence works until ${formatDate(status.licence_expires_at)}.` : "The licence doesn't expire."}{" "}
            It works on one computer at a time. Keep the key somewhere safe.
          </p>
        </div>
      );
    case "rejected":
      return (
        <div className="notice notice--warning" role="status">
          <p>Your application wasn't approved.</p>
          {status.reason && <p className="apply-reason">“{status.reason}”</p>}
        </div>
      );
    case "withdrawn":
      return (
        <p className="notice notice--warning" role="status">
          Your application was approved, but its licence has been withdrawn since. Contact the administrator if you need
          one.
        </p>
      );
  }
}

export function StatusView({ secret, justSent }: { secret: string; justSent: boolean }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const ask = useCallback(async () => {
    setBusy(true);
    try {
      setStatus(await askStatus(secret));
      setError(null);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }, [secret]);

  useEffect(() => {
    void ask();
  }, [ask]);
  const waiting = status?.status === "pending";
  useEffect(() => {
    if (!waiting) return;
    const timer = window.setInterval(() => void ask(), ASK_AGAIN_MS);
    return () => window.clearInterval(timer);
  }, [waiting, ask]);

  return (
    <div className="apply-status">
      {justSent && (
        <div className="notice notice--success" role="status">
          <p>Your application was sent. Keep this page's link: it is the only way to see how it went and to collect your key.</p>
          <p className="apply-key__row">
            <CopyButton size="normal" label="Copy link" text={() => Promise.resolve(statusLink(secret))} />
          </p>
        </div>
      )}
      {error !== null && (
        <p className="notice notice--error" role="alert">
          {explain(error)}
        </p>
      )}
      {!status && !error && <Loading label="Looking up your application…" />}
      {status && (
        <>
          <Outcome status={status} />
          <p className="field__help">
            Sent {formatDateTime(status.created_at)}
            {status.decided_at && `, decided ${formatDateTime(status.decided_at)}`}.
          </p>
        </>
      )}
      {(waiting || error !== null) && (
        <Button busy={busy} onClick={() => void ask()}>
          Check again
        </Button>
      )}
    </div>
  );
}
