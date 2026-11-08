// The inline confirmation a destructive action turns into: what will happen in plain words, an
// optional reason, a red Confirm and Cancel. Escape cancels. No dialogs. For the gravest actions,
// `confirmWord` must be typed before Confirm is enabled.

import { useState, type KeyboardEvent, type ReactNode } from "react";
import { describe } from "../errors";
import { Button } from "./Button";

type Props = {
  message: ReactNode;
  confirmLabel: string;
  withReason?: boolean;
  /** Confirm stays disabled until this is typed exactly. */
  confirmWord?: string;
  onConfirm: (reason: string | null) => Promise<void>;
  onCancel: () => void;
};

export function ConfirmStrip({ message, confirmLabel, withReason, confirmWord, onConfirm, onCancel }: Props) {
  const [reason, setReason] = useState("");
  const [typed, setTyped] = useState("");
  const unconfirmed = confirmWord !== undefined && typed !== confirmWord;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function confirm() {
    setBusy(true);
    setError(null);
    try {
      await onConfirm(reason.trim() || null);
    } catch (failure) {
      setError(describe(failure));
      setBusy(false);
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape" && !busy) {
      event.stopPropagation();
      onCancel();
    }
  }

  return (
    <div className="confirm-strip" role="group" aria-label={confirmLabel} onKeyDown={onKeyDown}>
      <p className="confirm-strip__message">{message}</p>
      {withReason && (
        <input
          className="confirm-strip__reason"
          aria-label="Reason (optional)"
          placeholder="Reason (optional)"
          value={reason}
          maxLength={500}
          onChange={(event) => setReason(event.target.value)}
          autoFocus
        />
      )}
      {confirmWord !== undefined && (
        <label className="confirm-strip__word">
          <span>
            Type <code>{confirmWord}</code> to confirm
          </span>
          <input value={typed} autoComplete="off" spellCheck={false} onChange={(event) => setTyped(event.target.value)} autoFocus={!withReason} />
        </label>
      )}
      <span className="confirm-strip__actions">
        <Button
          variant="danger"
          size="small"
          busy={busy}
          disabledReason={unconfirmed ? `Type ${confirmWord} to confirm` : null}
          onClick={() => void confirm()}
          autoFocus={!withReason && confirmWord === undefined}
        >
          {confirmLabel}
        </Button>
        <Button size="small" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </span>
      {error && (
        <p className="confirm-strip__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
