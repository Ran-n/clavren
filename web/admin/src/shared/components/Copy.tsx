// Copying to the clipboard, and a licence key that stays hidden until Reveal. A revealed key
// lives only in this component: it is gone when the admin leaves the page. `copyText` can be
// started before the text is known (the issue panel copies new keys while they are being issued),
// and `copyFailure` says why a copy failed, for every copy on the dashboard.

import { useEffect, useState } from "react";
import { api } from "../api";
import { describe } from "../errors";
import { HIDDEN_KEY } from "../keys";
import { Button } from "./Button";

const COPIED_FOR_MS = 2000;

/** Copies `text`, which may still be on its way. Call it from the click (or submit) itself: Safari,
 * and Firefox once its activation window has passed, refuse a copy that comes after an `await`.
 * A `ClipboardItem` given a promise is made at the click and filled in when the text arrives.
 * Rejects with the text's own error when the text never came, else with the browser's. */
export async function copyText(text: Promise<string> | string): Promise<void> {
  // only on HTTPS or localhost; elsewhere the browser has no clipboard to offer
  if (!navigator.clipboard) throw new DOMException("no clipboard", "NotAllowedError");
  const value = Promise.resolve(text);
  if (typeof ClipboardItem === "undefined" || !navigator.clipboard.write) {
    await navigator.clipboard.writeText(await value);
    return;
  }
  const blob = value.then((words) => new Blob([words], { type: "text/plain" }));
  // the browser reads `blob`; this keeps a failed text from also counting as unhandled
  blob.catch(() => undefined);
  try {
    await navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })]);
  } catch (failure) {
    await value; // the text's own error, when that is why
    throw failure;
  }
}

/** Why a copy failed, in words for the admin. */
export function copyFailure(failure: unknown): string {
  if (!(failure instanceof DOMException)) return describe(failure);
  return navigator.clipboard ? "The browser didn't allow copying. Copy it by hand." : "This browser only copies on HTTPS or localhost. Copy it by hand.";
}

/** A button that copies what `text` resolves to, then says so for a moment. */
export function CopyButton({ text, label, size = "small" }: { text: () => Promise<string>; label: string; size?: "small" | "normal" }) {
  const [state, setState] = useState<"idle" | "busy" | "copied">("idle");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (state !== "copied") return;
    const timer = window.setTimeout(() => setState("idle"), COPIED_FOR_MS);
    return () => window.clearTimeout(timer);
  }, [state]);

  async function copy() {
    setState("busy");
    setError(null);
    try {
      await copyText(text());
      setState("copied");
    } catch (failure) {
      setError(copyFailure(failure));
      setState("idle");
    }
  }

  return (
    <>
      <Button size={size} busy={state === "busy"} onClick={() => void copy()}>
        {state === "copied" ? "Copied" : label}
      </Button>
      {error && (
        <span className="inline-error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}

/** A licence key: hidden, with Reveal and Copy. Both are recorded in the audit log. */
export function KeyField({ licenceId, revoked }: { licenceId: string; revoked: boolean }) {
  const [key, setKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function fetchKey(): Promise<string> {
    const answer = await api<{ key: string }>("POST", `/licences/${licenceId}/reveal`, {});
    return answer.key;
  }

  async function reveal() {
    setBusy(true);
    setError(null);
    try {
      setKey(await fetchKey());
    } catch (failure) {
      setError(describe(failure));
    } finally {
      setBusy(false);
    }
  }

  if (revoked) {
    return <span className="key key--gone">Deleted when revoked</span>;
  }
  return (
    <span className="key-field">
      <code className="key">{key ?? HIDDEN_KEY}</code>
      {key ? (
        <Button size="small" variant="quiet" onClick={() => setKey(null)}>
          Hide
        </Button>
      ) : (
        <Button size="small" variant="quiet" busy={busy} onClick={() => void reveal()}>
          Reveal
        </Button>
      )}
      <CopyButton label="Copy" text={() => (key ? Promise.resolve(key) : fetchKey())} />
      {error && (
        <span className="inline-error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}
