// The application form: product (picked here, or fixed by the product's key page), name, email,
// an optional message, and the human check when the server has one. Once sent, the page moves to the
// application's private status link.

import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Button } from "../shared/components/Button";
import { Loading } from "../shared/components/Notice";
import { explain, loadForm, sendApplication, type Form } from "./publicApi";
import { Turnstile } from "./Turnstile";

/** `product` fixes the product (the key page shows its name and description itself); without it
 * the form offers a picker over the products that take applications. */
export function ApplyForm({ product: fixed, onSent }: { product?: string; onSent: (secret: string) => void }) {
  const [form, setForm] = useState<Form | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [product, setProduct] = useState(fixed ?? "");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [message, setMessage] = useState("");
  const [answer, setAnswer] = useState<string | null>(null);
  const [round, setRound] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const onAnswer = useCallback((value: string | null) => setAnswer(value), []);

  useEffect(() => {
    loadForm().then(setForm, setLoadError);
  }, []);

  if (loadError) return <p className="notice notice--error" role="alert">{explain(loadError)}</p>;
  if (!form) return <Loading label="Loading the form…" />;
  if (fixed !== undefined && !form.products.some((option) => option.code === fixed)) {
    // its applications closed between loading the key page and the form
    return <p className="notice notice--info">This product doesn't take applications at the moment.</p>;
  }
  if (!form.products[0]) {
    return <p className="notice notice--info">No product takes licence applications at the moment.</p>;
  }
  const first = form.products[0];
  if (!first) return null;
  const chosen = form.products.some((option) => option.code === product) ? product : first.code;
  const description = form.products.find((option) => option.code === chosen)?.description;
  const needsCheck = form.turnstile_site_key !== null && answer === null;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const sent = await sendApplication({ product: chosen, name, email, message, turnstileToken: answer });
      onSent(sent.secret);
    } catch (failure) {
      setError(failure);
      // a Turnstile answer works once: a new try needs a new check
      setRound((value) => value + 1);
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="apply-form" onSubmit={(event) => void submit(event)}>
      {fixed === undefined && (
        <div className="apply-form__product">
          {form.products.length > 1 ? (
          <label className="field">
            <span className="field__label">Product</span>
            <select value={chosen} onChange={(event) => setProduct(event.target.value)}>
              {form.products.map((option) => (
                <option key={option.code} value={option.code}>
                  {option.name}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <p className="apply-form__name">{first.name}</p>
        )}
          {description && <p className="field__help">{description}</p>}
        </div>
      )}
      <label className="field">
        <span className="field__label">Your name</span>
        <input value={name} maxLength={200} autoComplete="name" required onChange={(event) => setName(event.target.value)} />
      </label>
      <label className="field">
        <span className="field__label">Email</span>
        <input type="email" value={email} maxLength={320} autoComplete="email" required onChange={(event) => setEmail(event.target.value)} />
      </label>
      <label className="field">
        <span className="field__label">Message (optional)</span>
        <textarea value={message} maxLength={2000} rows={4} placeholder="What you'll use it for, your school or company…" onChange={(event) => setMessage(event.target.value)} />
      </label>
      {form.turnstile_site_key && <Turnstile siteKey={form.turnstile_site_key} onAnswer={onAnswer} round={round} />}
      {error !== null && (
        <p className="notice notice--error" role="alert">
          {explain(error)}
        </p>
      )}
      <Button type="submit" variant="primary" size="large" busy={busy} disabledReason={needsCheck ? "Complete the check above first" : null}>
        Send application
      </Button>
      <p className="field__help">
        Your name, email and message go to the administrator only. After sending you get a private link to see how your
        application went.
      </p>
    </form>
  );
}
