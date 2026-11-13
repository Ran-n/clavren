// The first page: products, licences by state, computers, the backups (with a warning when the
// last one is too old), and the server: its version and the last check for a newer release.

import { useEffect, useState } from "react";
import { get } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ErrorNotice, Loading, Notice } from "../shared/components/Notice";
import { formatBytes, formatDateTime, formatNumber, formatPrice } from "../shared/format";
import { useTitle } from "../shared/hooks";
import { Link } from "../shared/router";
import type { BackupRun, Overview, Updates } from "../shared/types";
import { useUpdates } from "../shared/updates";

function Stat({ value, label, to }: { value: number | string; label: string; to?: string }) {
  const content = (
    <>
      <span className="stat__value">{typeof value === "number" ? formatNumber(value) : value}</span>
      <span className="stat__label">{label}</span>
    </>
  );
  return to ? (
    <Link to={to} className="stat stat--link">
      {content}
    </Link>
  ) : (
    <div className="stat">{content}</div>
  );
}

/** What keys were charged, one line of figures per currency; test licences left out. */
function Charged({ overview }: { overview: Overview }) {
  const sums = overview.charged.length > 0 ? overview.charged : [{ currency: overview.default_currency, last_30_days: 0, total: 0 }];
  return (
    <>
      <h2>Charged</h2>
      {sums.map((sum) => (
        <div className="stats" key={sum.currency}>
          <Stat value={formatPrice({ amount: sum.last_30_days, currency: sum.currency })} label="Charged, last 30 days" />
          <Stat value={formatPrice({ amount: sum.total, currency: sum.currency })} label="Charged in total" />
        </div>
      ))}
      <p className="muted">
        {overview.unpriced > 0 && `${overview.unpriced === 1 ? "1 key has" : `${formatNumber(overview.unpriced)} keys have`} no recorded price. `}
        Deleted licences aren't counted.
      </p>
    </>
  );
}

function BackupWarning({ overview }: { overview: Overview }) {
  if (!overview.backup_overdue) return null;
  return (
    <Notice tone="warning">
      {overview.last_backup
        ? `The last backup finished ${formatDateTime(overview.last_backup.finished_at)}, more than 48 hours ago.`
        : "There is no backup yet."}{" "}
      Check that the backup drive is mounted and run <code>sudo systemctl start clavren-backup</code>; see
      backup-and-recovery.md.
    </Notice>
  );
}

function Backups({ runs }: { runs: BackupRun[] }) {
  if (runs.length === 0) return <p className="muted">No backups recorded yet.</p>;
  return (
    <div className="table-wrap">
      <table className="table" aria-label="Backups">
        <thead>
          <tr>
            <th scope="col">Finished</th>
            <th scope="col">Archive</th>
            <th scope="col">Size</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((run) => (
            <tr key={run.id}>
              <td data-label="Finished">{formatDateTime(run.finished_at)}</td>
              <td data-label="Archive">
                <code>{run.archive_name}</code>
              </td>
              <td data-label="Size">{formatBytes(run.size_bytes)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** A sentence from the update check's reason, which comes without a final full stop. */
function sentence(text: string): string {
  return /[.!?]$/.test(text) ? text : `${text}.`;
}

function lastCheck(updates: Updates): string {
  if (updates.checking === "off") return "Checking for updates is off.";
  if (!updates.last_check) return "Not checked yet.";
  const { checked_at: at, error } = updates.last_check;
  if (error) return `Last check failed at ${formatDateTime(at)}: ${sentence(error)}`;
  const newest = updates.available ? `Clavren ${updates.available.version} is available.` : "This is the newest release.";
  return `Last checked ${formatDateTime(at)}. ${newest}`;
}

function Server() {
  const { updates, error, retry, check, checkNow } = useUpdates();
  const [checkError, setCheckError] = useState<unknown>(null);
  const failed = error ? (
    <ErrorNotice error={error}>
      <Button size="small" variant="quiet" onClick={retry}>
        Try again
      </Button>
    </ErrorNotice>
  ) : null;
  if (!updates) return failed ?? <Loading />;
  return (
    <div className="server-facts">
      <p>Version {updates.running_version}</p>
      <p>
        {lastCheck(updates)}{" "}
        <Button
          size="small"
          variant="quiet"
          busy={check === "waiting"}
          disabledReason={updates.checking === "off" ? "Checking for updates is off" : null}
          onClick={() => {
            setCheckError(null);
            checkNow().catch(setCheckError);
          }}
        >
          Check now
        </Button>
      </p>
      {check === "slow" && (
        <Notice tone="warning">
          The check is taking longer than usual; see <code>journalctl -u clavren-update-check</code>.
        </Notice>
      )}
      <ErrorNotice error={checkError} />
      {failed}
    </div>
  );
}

export function OverviewPage() {
  useTitle("Overview");
  const [overview, setOverview] = useState<Overview | null>(null);
  const [backups, setBackups] = useState<BackupRun[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    get<Overview>("/overview").then(setOverview, setError);
    get<{ data: BackupRun[] }>("/backups").then((answer) => setBackups(answer.data), setError);
  }, []);

  return (
    <section className="page">
      <h1>Overview</h1>
      <ErrorNotice error={error} />
      {!overview && !error && <Loading />}
      {overview && (
        <>
          <BackupWarning overview={overview} />
          {overview.pending_applications > 0 && (
            <Notice tone="info">
              {overview.pending_applications === 1
                ? "1 licence application is waiting."
                : `${formatNumber(overview.pending_applications)} licence applications are waiting.`}{" "}
              <Link to="/applications">Review applications</Link>
            </Notice>
          )}
          <h2>Licences</h2>
          <div className="stats">
            <Stat value={overview.unused} label="Unused" to="/licences?state=unused&not_test=true" />
            <Stat value={overview.redeemed} label="In use" to="/licences?state=redeemed&not_test=true" />
            <Stat value={overview.revoked} label="Revoked" to="/licences?state=revoked&not_test=true" />
          </div>
          <p className="muted">Test licences, their computers and their checks aren't counted here.</p>
          <Charged overview={overview} />
          <h2>Computers and products</h2>
          <div className="stats">
            <Stat value={overview.active_computers} label="Computers using a licence" to="/activations" />
            <Stat value={overview.computers_checked_last_24h} label="Computers checked in, last 24 hours" />
            <Stat value={overview.checks_last_24h} label="Checks, last 24 hours" />
            <Stat value={overview.active_products} label="Active products" to="/products" />
            <Stat value={overview.inactive_products} label="Inactive products" to="/products" />
          </div>
        </>
      )}
      <h2>Backups</h2>
      {backups ? <Backups runs={backups} /> : !error && <Loading />}
      <h2>Server</h2>
      <Server />
    </section>
  );
}
