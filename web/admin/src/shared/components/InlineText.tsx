// A value edited where it is shown: click it, edit, then Enter or leaving the field saves and
// Escape cancels. In a multi-line field Enter is a new line and Ctrl+Enter saves. A failed save
// keeps the field open with the reason under it.

import { useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { describe } from "../errors";

type Props = {
  value: string | null;
  /** Saves the new value; `null` when the field was emptied. Rejects to keep the field open. */
  onSave: (value: string | null) => Promise<void>;
  label: string;
  placeholder?: string;
  multiline?: boolean;
  type?: "text" | "email" | "date";
  display?: ReactNode;
  maxLength?: number;
};

export function InlineText({ value, onSave, label, placeholder = "Add", multiline, type = "text", display, maxLength }: Props) {
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const finishing = useRef(false);

  function start() {
    setDraft(value ?? "");
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
    const next = draft.trim() === "" ? null : draft.trim();
    if (next === (value ?? null)) return cancel();
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

  function onKeyDown(event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
    } else if (event.key === "Enter" && (!multiline || event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void commit();
    }
  }

  if (draft === null) {
    return (
      <button type="button" className="inline-value" onClick={start} aria-label={`Edit ${label}`}>
        {value ? (display ?? value) : <span className="inline-value__placeholder">{placeholder}</span>}
      </button>
    );
  }
  const shared = {
    "aria-label": label,
    "aria-invalid": error ? true : undefined,
    value: draft,
    disabled: saving,
    autoFocus: true,
    maxLength,
    onKeyDown,
    onBlur: () => void commit(),
    className: "inline-input",
  };
  return (
    <span className="inline-editor">
      {multiline ? (
        <textarea {...shared} rows={6} onChange={(event) => setDraft(event.target.value)} />
      ) : (
        <input {...shared} type={type} onChange={(event) => setDraft(event.target.value)} />
      )}
      {saving && <span className="inline-editor__status">Saving…</span>}
      {multiline && !saving && <span className="inline-editor__hint">Ctrl+Enter saves, Escape cancels</span>}
      {error && (
        <span className="inline-editor__error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}
