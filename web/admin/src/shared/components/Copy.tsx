// Copying to the clipboard, and a licence key that stays hidden until Reveal. A revealed key
// lives only in this component: it is gone when the admin leaves the page.

import { useEffect, useState } from "react";
import { api } from "../api";
import { describe } from "../errors";
import { HIDDEN_KEY } from "../keys";
import { Button } from "./Button";

const COPIED_FOR_MS = 2000;

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
      // only on HTTPS or localhost; elsewhere the browser has no clipboard to offer
      if (!navigator.clipboard) throw new DOMException("no clipboard", "NotAllowedError");
      await navigator.clipboard.writeText(await text());
      setState("copied");
    } catch (failure) {
      setError(
        failure instanceof DOMException
          ? navigator.clipboard
            ? "The browser didn't allow copying. Copy it by hand."
            : "This browser only copies on HTTPS or localhost. Copy it by hand."
          : describe(failure),
      );
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
