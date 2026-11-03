// The admin's own account: password, authenticator app, recovery codes, other sessions.

import { useTitle } from "../shared/hooks";
import { useSignedIn } from "../shared/session";
import { AuthenticatorSection } from "./AuthenticatorSection";
import { PasswordSection } from "./PasswordSection";
import { RecoveryCodesSection } from "./RecoveryCodesSection";
import { SessionsSection } from "./SessionsSection";

export function AccountPage() {
  useTitle("Account");
  const session = useSignedIn();
  return (
    <section className="page">
      <h1>Account</h1>
      <p className="muted">Signed in as {session.admin.username}.</p>
      <PasswordSection />
      <AuthenticatorSection />
      <RecoveryCodesSection />
      <SessionsSection />
    </section>
  );
}
