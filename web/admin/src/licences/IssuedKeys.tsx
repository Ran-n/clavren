// The keys just issued, shown in the Issue panel: each key with a copy button, copy buttons for
// all of them and the message, a CSV download, and whether copying worked. They stay on screen
// until the panel closes.

import { useState } from "react";
import { download } from "../shared/api";
import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { plural } from "../shared/errors";
import type { Customer, Licence } from "../shared/types";
import { csvName } from "./licenceApi";

export type Issued = { licences: (Licence & { key: string })[]; customer: Customer | null; message: string | null };
/** How copying the new keys went: copied, the reason it failed, or still under way (`null`). */
export type Copied = "copied" | { failed: string } | null;

/** The new keys, one per line, in the order shown. */
export const keysText = (issued: Issued) => issued.licences.map((licence) => licence.key).join("\n");

function CopiedNotice({ copied, count }: { copied: Copied; count: number }) {
  if (copied === null) return null;
  if (copied === "copied") return <Notice tone="success">{count === 1 ? "The key is on the clipboard." : `The ${count} keys are on the clipboard.`}</Notice>;
  return <Notice tone="warning">{copied.failed}</Notice>;
}

export function IssuedKeys({ issued, copied, onMore, onClose }: { issued: Issued; copied: Copied; onMore: () => void; onClose: () => void }) {
  const [error, setError] = useState<unknown>(null);
  const [downloading, setDownloading] = useState(false);
  const ids = issued.licences.map((licence) => licence.id);
  const count = issued.licences.length;
  return (
    <div className="issue-result">
      <h2>
        {plural(issued.licences.length, issued.licences[0]?.is_test ? "test licence" : "licence")} issued
      </h2>
      <CopiedNotice copied={copied} count={count} />
      <ul className="issue-result__keys" aria-label="New keys">
        {issued.licences.map((licence) => (
          <li key={licence.id}>
            <code className="key">{licence.key}</code>
            {count > 1 && <CopyButton label="Copy" text={() => Promise.resolve(licence.key)} />}
          </li>
        ))}
      </ul>
      <div className="button-row">
        <CopyButton size="normal" label={count === 1 ? "Copy key" : "Copy keys"} text={() => Promise.resolve(keysText(issued))} />
        {issued.message && <CopyButton size="normal" label="Copy message" text={() => Promise.resolve(issued.message as string)} />}
        <Button
          busy={downloading}
          onClick={() => {
            setError(null);
            setDownloading(true);
            download("/licences/export", { ids }, csvName())
              .catch(setError)
              .finally(() => setDownloading(false));
          }}
        >
          Download CSV
        </Button>
        <Button variant="quiet" onClick={onMore}>
          Issue more
        </Button>
        <Button variant="quiet" onClick={onClose}>
          Close
        </Button>
      </div>
      <ErrorNotice error={error} />
    </div>
  );
}
