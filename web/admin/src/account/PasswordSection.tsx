// Changing the password. Every other session is signed out by the server.

import { useState } from "react";
import { api } from "../shared/api";
import { formatNumber } from "../shared/format";
import { AccountForm, TextField } from "./Form";

const MIN_LENGTH = 12;
const MAX_LENGTH = 1024;
const WORDING = { invalid_credentials: "The current password is wrong." };

function problem(next: string, repeat: string): string | null {
  // characters, as the server counts them, not UTF-16 units
  const length = [...next].length;
  if (length > 0 && length < MIN_LENGTH) return `The new password needs at least ${MIN_LENGTH} characters`;
  if (length > MAX_LENGTH) return `The new password can have at most ${formatNumber(MAX_LENGTH)} characters`;
  if (repeat && next !== repeat) return "The two new passwords are different";
  return null;
}

export function PasswordSection() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");

  async function change() {
    await api("POST", "/account/password", { current_password: current, new_password: next });
    setCurrent("");
    setNext("");
    setRepeat("");
    return "Password changed. Your other sessions were signed out.";
  }

  return (
    <AccountForm title="Password" description="At least 12 characters. Changing it signs you out everywhere else." submitLabel="Change password" onSubmit={change} disabledReason={problem(next, repeat)} wording={WORDING}>
      <TextField label="Current password" type="password" autoComplete="current-password" value={current} onChange={setCurrent} />
      <TextField label="New password" type="password" autoComplete="new-password" value={next} onChange={setNext} />
      <TextField label="New password again" type="password" autoComplete="new-password" value={repeat} onChange={setRepeat} help={problem(next, repeat) ?? undefined} />
    </AccountForm>
  );
}
