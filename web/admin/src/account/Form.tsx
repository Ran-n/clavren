// The pieces the account forms share: a titled card with a form that shows its own busy state,
// error and success message.

import { useState, type FormEvent, type ReactNode } from "react";
import { Button } from "../shared/components/Button";
import { ErrorNotice, Notice } from "../shared/components/Notice";

type Props = {
  title: string;
  description: ReactNode;
  submitLabel: string;
  /** Does the work; resolves to the success message, or rejects with the error to show. */
  onSubmit: () => Promise<string | null>;
  children: ReactNode;
  disabledReason?: string | null;
};

export function AccountForm({ title, description, submitLabel, onSubmit, children, disabledReason }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [done, setDone] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      setDone(await onSubmit());
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="card account-card" aria-label={title} onSubmit={(event) => void submit(event)}>
      <h2>{title}</h2>
      <p className="muted">{description}</p>
      {children}
      <ErrorNotice error={error} />
      {done && <Notice tone="success">{done}</Notice>}
      <div className="button-row">
        <Button type="submit" busy={busy} disabledReason={disabledReason}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}

type FieldProps = { label: string; value: string; onChange: (value: string) => void; type?: string; autoComplete?: string; help?: string };

export function TextField({ label, value, onChange, type = "text", autoComplete, help }: FieldProps) {
  return (
    <label className="field">
      <span className="field__label">{label}</span>
      <input type={type} value={value} autoComplete={autoComplete} onChange={(event) => onChange(event.target.value)} required />
      {help && <span className="field__help">{help}</span>}
    </label>
  );
}
