// The notice at the top of every page when a newer Clavren release is out: what it is, a link to
// what's new, and Update, which turns into a confirmation in place. While an update runs it says
// how far it got; afterwards, that it worked (once) or why it didn't.

import { useState } from "react";
import { Button } from "../shared/components/Button";
import { ConfirmStrip } from "../shared/components/ConfirmStrip";
import { Notice } from "../shared/components/Notice";
import type { UpdateStage, Updates } from "../shared/types";
import { useUpdates, type Installing } from "../shared/updates";

const STAGES: Record<UpdateStage, string> = {
  starting: "starting",
  downloading: "downloading",
  checking: "checking the download",
  backing_up: "backing up",
  installing: "installing",
  restarting: "restarting",
};

function Progress({ installing, stage }: { installing: Installing; stage: UpdateStage | null }) {
  if (installing.phase === "slow") {
    return (
      <Notice tone="warning">
        The update is taking longer than usual; see <code>journalctl -u clavren-update</code> on the server.
      </Notice>
    );
  }
  const step = installing.phase === "restarting" ? STAGES.restarting : stage ? STAGES[stage] : null;
  return (
    <Notice>
      Updating to {installing.version}…{step && <span className="muted"> ({step})</span>}
    </Notice>
  );
}

function whyNot(updates: Updates): string | null {
  if (updates.can_install) return null;
  if (!updates.available?.installable) return "Its files aren't uploaded yet";
  return "An update is running";
}

export function UpdateNotice() {
  const { updates, installing, install, updatedTo } = useUpdates();
  const [confirming, setConfirming] = useState(false);

  if (updatedTo && updates?.running_version === updatedTo) {
    return <Notice tone="success">Updated to {updatedTo}.</Notice>;
  }
  if (!updates) return null;
  if (installing) return <Progress installing={installing} stage={updates.update?.stage ?? null} />;
  const available = updates.available;
  if (!available) return null;

  const failed = updates.update?.state === "failed" && updates.update.version === available.version ? updates.update : null;
  const whatsNew = available.release_url && (
    <a href={available.release_url} target="_blank" rel="noreferrer">
      What's new
    </a>
  );
  const text = available.installable
    ? `Clavren ${available.version} is available (this server runs ${updates.running_version}).`
    : `Clavren ${available.version} is being published; it can be installed once its files are uploaded.`;

  return (
    <Notice tone={failed ? "warning" : "info"}>
      <span className="update-notice">
        <span className="update-notice__text">
          {text}
          {failed && (
            <>
              {" "}
              The update failed: {failed.message ?? "see journalctl -u clavren-update on the server."}
            </>
          )}
        </span>
        <span className="update-notice__actions">
          {whatsNew}
          {updates.updater_installed && !confirming && (
            <Button size="small" variant="primary" disabledReason={whyNot(updates)} onClick={() => setConfirming(true)}>
              Update to {available.version}
            </Button>
          )}
        </span>
        {confirming && (
          <ConfirmStrip
            message={`Clavren ${updates.backup_drive_mounted ? "backs up, installs" : "installs"} ${available.version} and restarts. Apps can't activate or check in for about a minute; they keep working offline meanwhile.`}
            confirmLabel="Confirm"
            acknowledge={updates.backup_drive_mounted ? undefined : "Update without a backup (the backup drive isn't mounted)"}
            onConfirm={async () => {
              await install(available.version, !updates.backup_drive_mounted);
              setConfirming(false);
            }}
            onCancel={() => setConfirming(false)}
          />
        )}
      </span>
    </Notice>
  );
}
