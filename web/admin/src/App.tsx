// The dashboard: the sign-in page until there is a session, then the page the address names,
// starting from the last page the admin had open.

import { useEffect, useRef } from "react";
import { AccountPage } from "./account/AccountPage";
import { ActivationsPage } from "./activations/ActivationsPage";
import { AuditPage } from "./audit/AuditPage";
import { CustomersPage } from "./customers/CustomersPage";
import { LicenceDetailPage } from "./licences/LicenceDetailPage";
import { LicencesPage } from "./licences/LicencesPage";
import { LoginPage } from "./login/LoginPage";
import { OverviewPage } from "./overview/OverviewPage";
import { ProductsPage } from "./products/ProductsPage";
import { Loading, Notice } from "./shared/components/Notice";
import { PreferencesProvider, usePreferences } from "./shared/preferences";
import { Link, navigate, useLocation } from "./shared/router";
import { SessionProvider, useSession } from "./shared/session";
import { Layout } from "./shell/Layout";

const LICENCE_PATH = /^\/licences\/([0-9a-f-]{36})$/;

function Page() {
  const { path } = useLocation();
  const licence = LICENCE_PATH.exec(path)?.[1];
  if (licence) return <LicenceDetailPage id={licence} key={licence} />;
  switch (path) {
    case "/":
      return <OverviewPage />;
    case "/licences":
      return <LicencesPage />;
    case "/customers":
      return <CustomersPage />;
    case "/products":
      return <ProductsPage />;
    case "/activations":
      return <ActivationsPage />;
    case "/audit":
      return <AuditPage />;
    case "/account":
      return <AccountPage />;
    default:
      return (
        <Notice tone="warning">
          There is no page here. <Link to="/">Go to the Overview</Link>.
        </Notice>
      );
  }
}

/** After signing in at the root, go back to the page the admin was last on. */
function RestoreLastPage() {
  const { preferences } = usePreferences();
  const restored = useRef(false);
  useEffect(() => {
    if (restored.current) return;
    restored.current = true;
    const last = preferences.last_page;
    if (window.location.pathname === "/" && last && last !== "/") navigate(last, { replace: true });
  }, [preferences.last_page]);
  return null;
}

function SignedIn() {
  return (
    <PreferencesProvider loading={<Loading label="Loading your settings…" />}>
      <RestoreLastPage />
      <Layout>
        <Page />
      </Layout>
    </PreferencesProvider>
  );
}

function Root() {
  const { state } = useSession();
  if (state.status === "loading") return <Loading />;
  if (state.status === "signed_out") return <LoginPage ended={state.ended} error={state.error} />;
  return <SignedIn />;
}

export function App() {
  return (
    <SessionProvider>
      <Root />
    </SessionProvider>
  );
}
