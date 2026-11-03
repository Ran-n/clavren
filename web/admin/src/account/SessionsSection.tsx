// Signing out every other browser this admin is signed in on.

import { api } from "../shared/api";
import { plural } from "../shared/errors";
import { AccountForm } from "./Form";

export function SessionsSection() {
  async function revokeOthers() {
    const { revoked } = await api<{ revoked: number }>("POST", "/session/revoke-others", {});
    return revoked === 0 ? "You weren't signed in anywhere else." : `Signed out ${plural(revoked, "other session")}.`;
  }
  return (
    <AccountForm title="Other sessions" description="Sign out every other browser where you are signed in. This one stays signed in." submitLabel="Sign out everywhere else" onSubmit={revokeOthers}>
      {null}
    </AccountForm>
  );
}
