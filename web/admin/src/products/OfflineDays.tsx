// A product's offline limit, edited where it is shown: click the badge, type a whole number of
// days, then Enter or leaving the field saves and Escape cancels, as every inline value does. A
// number outside 2 to 3,650 is refused in place and nothing is saved.

import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Badge } from "../shared/components/Badge";
import { describe, plural } from "../shared/errors";

export const MIN_OFFLINE_DAYS = 2;
export const MAX_OFFLINE_DAYS = 3650;
const OUT_OF_RANGE = "Enter 2 to 3,650 days.";

/** The days typed, when they are a whole number in range. */
export function parseOfflineDays(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const days = Number(trimmed);
  return days >= MIN_OFFLINE_DAYS && days <= MAX_OFFLINE_DAYS ? days : null;
}

type Props = {
  days: number;
  /** Saves (or asks to confirm) the new limit. Rejects to keep the field open with the reason. */
  onSave: (days: number) => Promise<void>;
};

export function OfflineDays({ days, onSave }: Props) {
  const valueId = useId();
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finishing = useRef(false);

  function start() {
    setDraft(String(days));
    setError(null);
    finishing.current = false;
  }

  function cancel() {
    finishing.current = true;
    setDraft(null);
    setError(null);
  }

  async function commit() {
    if (draft === null || finishing.current) return;
    const next = parseOfflineDays(draft);
    if (next === null) return setError(OUT_OF_RANGE);
    if (next === days) return cancel();
    finishing.current = true;
    setSaving(true);
    try {
      await onSave(next);
      setDraft(null);
      setError(null);
    } catch (failure) {
      setError(describe(failure));
      finishing.current = false;
    } finally {
      setSaving(false);
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
    } else if (event.key === "Enter") {
      event.preventDefault();
      void commit();
    }
  }

  if (draft === null) {
    return (
      <button type="button" className="inline-value" onClick={start} aria-label="Edit days offline" aria-describedby={valueId}>
        <span id={valueId}>
          <Badge tone="neutral">{plural(days, "day")}</Badge>
        </span>
      </button>
    );
  }
  return (
    <span className="inline-editor">
      <input
        className="inline-input inline-input--days"
        aria-label="days offline"
        aria-invalid={error ? true : undefined}
        inputMode="numeric"
        value={draft}
        disabled={saving}
        autoFocus
        onKeyDown={onKeyDown}
        onBlur={() => void commit()}
        onChange={(event) => setDraft(event.target.value)}
      />
      {saving && <span className="inline-editor__status">Saving…</span>}
      {error && (
        <span className="inline-editor__error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}
