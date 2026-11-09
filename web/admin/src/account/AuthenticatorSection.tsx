// Moving to a new authenticator app: get a new secret (nothing changes yet), add it to the app,
// then confirm with the password and a code from the new app. Until then the old app works.

import { useState } from "react";
import { api } from "../shared/api";
import { Button } from "../shared/components/Button";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { AccountForm, TextField } from "./Form";

type NewSecret = { secret: string; uri: string; qr_svg: string };

function qrSource(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

export function AuthenticatorSection() {
  const [fresh, setFresh] = useState<NewSecret | null>(null);
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // Shown after confirming, when the section is back to its first state.
  const [done, setDone] = useState<string | null>(null);

  async function start() {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      setFresh(await api<NewSecret>("POST", "/account/totp/secret"));
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  async function confirm() {
    await api("PUT", "/account/totp", { password, secret: fresh?.secret, code });
    setFresh(null);
    setPassword("");
    setCode("");
    setDone("Done: from now on only the new app's codes work.");
    return null;
  }

  if (!fresh) {
    return (
      <section className="card account-card" aria-label="Authenticator app">
        <h2>Authenticator app</h2>
        <p className="muted">Move to a new phone or app. The current one keeps working until you confirm the new one.</p>
        <ErrorNotice error={error} />
        {done && <Notice tone="success">{done}</Notice>}
        <div className="button-row">
          <Button busy={busy} onClick={() => void start()}>
            Move to a new authenticator app
          </Button>
        </div>
      </section>
    );
  }
  return (
    <AccountForm
      title="Authenticator app"
      description="Scan the code with the new app (or type the key in by hand), then confirm with your password and a code from the new app."
      submitLabel="Confirm the new app"
      onSubmit={confirm}
      wording={{ invalid_credentials: "The password is wrong." }}
    >
      <img className="qr" src={qrSource(fresh.qr_svg)} alt="QR code for the authenticator app" width={200} height={200} />
      <p>
        Key: <code className="secret">{fresh.secret}</code>
      </p>
      <TextField label="Password" type="password" autoComplete="current-password" value={password} onChange={setPassword} />
      <TextField label="Code from the new app" autoComplete="one-time-code" value={code} onChange={setCode} />
      <Button variant="quiet" size="small" onClick={() => setFresh(null)}>
        Cancel
      </Button>
    </AccountForm>
  );
}
