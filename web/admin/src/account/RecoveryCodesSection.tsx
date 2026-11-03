// New recovery codes: shown once, and every old code stops working.

import { useState } from "react";
import { api } from "../shared/api";
import { CopyButton } from "../shared/components/Copy";
import { AccountForm, TextField } from "./Form";

export function RecoveryCodesSection() {
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);

  async function regenerate() {
    const answer = await api<{ recovery_codes: string[] }>("POST", "/account/recovery-codes", { password, code });
    setCodes(answer.recovery_codes);
    setPassword("");
    setCode("");
    return "New codes made. Store them somewhere safe and offline: they are shown only now, and the old ones no longer work.";
  }

  return (
    <AccountForm
      title="Recovery codes"
      description="Each one signs you in once instead of an authenticator code. Making new ones cancels every old one."
      submitLabel="Make new recovery codes"
      onSubmit={regenerate}
    >
      {codes && (
        <div className="recovery-codes">
          <ol aria-label="New recovery codes">
            {codes.map((recovery) => (
              <li key={recovery}>
                <code>{recovery}</code>
              </li>
            ))}
          </ol>
          <CopyButton label="Copy all" text={() => Promise.resolve(codes.join("\n"))} />
        </div>
      )}
      <TextField label="Password" type="password" autoComplete="current-password" value={password} onChange={setPassword} />
      <TextField label="Code from the authenticator app" autoComplete="one-time-code" value={code} onChange={setCode} />
    </AccountForm>
  );
}
