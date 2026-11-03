// Every computer a licence was activated on, newest first, with why each one stopped.

import { useEffect, useState } from "react";
import { get } from "../shared/api";
import { ErrorNotice, Loading } from "../shared/components/Notice";
import { formatDateTime } from "../shared/format";
import type { Activation, Page } from "../shared/types";
import { activationState } from "../activations/activationState";

export function ActivationHistory({ licenceId }: { licenceId: string }) {
  const [activations, setActivations] = useState<Activation[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    get<Page<Activation>>(`/activations?licence=${licenceId}&limit=100`).then((page) => setActivations(page.data), setError);
  }, [licenceId]);

  if (error) return <ErrorNotice error={error} />;
  if (!activations) return <Loading />;
  if (activations.length === 0) return <p className="muted">Not activated on any computer yet.</p>;
  return (
    <div className="table-wrap">
      <table className="table" aria-label="Computers">
        <thead>
          <tr>
            <th scope="col">Computer</th>
            <th scope="col">App version</th>
            <th scope="col">Activated</th>
            <th scope="col">Last check</th>
            <th scope="col">State</th>
          </tr>
        </thead>
        <tbody>
          {activations.map((activation) => (
            <tr key={activation.id}>
              <td data-label="Computer">
                <span className="stacked">
                  <span>{activation.device_name}</span>
                  <span className="muted">{activation.platform}</span>
                </span>
              </td>
              <td data-label="App version">{activation.app_version}</td>
              <td data-label="Activated">{formatDateTime(activation.activated_at)}</td>
              <td data-label="Last check">{formatDateTime(activation.last_checked_at)}</td>
              <td data-label="State">{activationState(activation)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
