// Signing in: username, password, and the authenticator app's code (or a recovery code).

import { useState, type FormEvent } from "react";
import { api } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { useTitle } from "../shared/hooks";
import { useSession, type Session } from "../shared/session";

export function LoginPage({ ended, error: loadError }: { ended: boolean; error?: unknown }) {
  useTitle("Sign in");
  const { signedIn } = useSession();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(loadError ?? null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      signedIn(await api<Session>("POST", "/session", { username, password, code }));
    } catch (failure) {
      setError(failure);
      setCode("");
      setBusy(false);
    }
  }

  return (
    <main className="login">
      <form className="login__form card" onSubmit={(event) => void submit(event)}>
        <h1 className="login__title">Clavren</h1>
        {ended && !error && <Notice tone="warning">Your session has ended. Sign in again.</Notice>}
        <ErrorNotice error={error} />
        <label className="field">
          <span className="field__label">Username</span>
          <input autoComplete="username" value={username} onChange={(event) => setUsername(event.target.value)} required autoFocus />
        </label>
        <label className="field">
          <span className="field__label">Password</span>
          <input type="password" autoComplete="current-password" value={password} onChange={(event) => setPassword(event.target.value)} required />
        </label>
        <label className="field">
          <span className="field__label">Code from the authenticator app</span>
          <input
            autoComplete="one-time-code"
            inputMode="text"
            value={code}
            onChange={(event) => setCode(event.target.value)}
            required
            aria-describedby="code-help"
          />
          <span className="field__help" id="code-help">
            Or one of your recovery codes.
          </span>
        </label>
        <Button type="submit" variant="primary" size="large" busy={busy}>
          Sign in
        </Button>
      </form>
    </main>
  );
}
