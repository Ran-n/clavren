// The first page: products, licences by state, computers, and the backups, with a warning when
// the last backup is too old.

import { useEffect, useState } from "react";
import { get } from "../shared/api";
import { ErrorNotice, Loading, Notice } from "../shared/components/Notice";
import { formatBytes, formatDateTime } from "../shared/format";
import { useTitle } from "../shared/hooks";
import { Link } from "../shared/router";
import type { BackupRun, Overview } from "../shared/types";

function Stat({ value, label, to }: { value: number; label: string; to?: string }) {
  const content = (
    <>
      <span className="stat__value">{value.toLocaleString()}</span>
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
          <h2>Licences</h2>
          <div className="stats">
            <Stat value={overview.unused} label="Unused" to="/licences?state=unused" />
            <Stat value={overview.redeemed} label="In use" to="/licences?state=redeemed" />
            <Stat value={overview.revoked} label="Revoked" to="/licences?state=revoked" />
          </div>
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
    </section>
  );
}
