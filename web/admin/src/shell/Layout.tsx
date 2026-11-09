// The frame around every page once signed in: the navigation, the theme, who is signed in, a
// notice when settings can't be saved, one when a newer Clavren release is out, and one after
// another admin's session took over this page.

import { useEffect, useState, type ReactNode } from "react";
import { Button } from "../shared/components/Button";
import { ErrorNotice, Notice } from "../shared/components/Notice";
import { usePreferences, type Theme } from "../shared/preferences";
import { Link, useLocation } from "../shared/router";
import { useSession, useSignedIn } from "../shared/session";
import { UpdateNotice } from "./UpdateNotice";

const PAGES = [
  { path: "/", label: "Overview" },
  { path: "/licences", label: "Licences" },
  { path: "/customers", label: "Customers" },
  { path: "/products", label: "Products" },
  { path: "/activations", label: "Activations" },
  { path: "/audit", label: "Audit log" },
  { path: "/account", label: "Account" },
];

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "System theme" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

function isCurrent(page: string, path: string): boolean {
  return page === "/" ? path === "/" : path === page || path.startsWith(`${page}/`);
}

export function Layout({ children }: { children: ReactNode }) {
  const { path, search } = useLocation();
  const { preferences, update, flush, saveError } = usePreferences();
  const { state, signOut } = useSession();
  const session = useSignedIn();
  const [signOutError, setSignOutError] = useState<unknown>(null);
  // the switch is news on the page it happened on, until the admin goes elsewhere
  const switched = state.status === "signed_in" ? state.switched : undefined;
  const [switchedOn, setSwitchedOn] = useState<string | null>(switched ? path : null);
  useEffect(() => {
    if (switchedOn !== null && path !== switchedOn) setSwitchedOn(null);
  }, [path, switchedOn]);

  useEffect(() => {
    const page = path + (search.size ? `?${search}` : "");
    update((current) => (current.last_page === page ? current : { ...current, last_page: page }));
  }, [path, search, update]);

  return (
    <div className="app">
      <header className="app-header">
        <Link to="/" className="app-header__brand">
          Clavren
        </Link>
        <nav className="app-nav" aria-label="Pages">
          {PAGES.map((page) => (
            <Link key={page.path} to={page.path} current={isCurrent(page.path, path)} className="app-nav__link">
              {page.label}
            </Link>
          ))}
        </nav>
        <div className="app-header__end">
          <select
            aria-label="Theme"
            value={preferences.theme}
            onChange={(event) => update((current) => ({ ...current, theme: event.target.value as Theme }))}
          >
            {THEMES.map((theme) => (
              <option key={theme.value} value={theme.value}>
                {theme.label}
              </option>
            ))}
          </select>
          <span className="app-header__user">{session.admin.username}</span>
          <Button size="small" variant="quiet" onClick={() => flush().then(signOut).catch(setSignOutError)}>
            Sign out
          </Button>
        </div>
      </header>
      <main className="app-main">
        {switchedOn !== null && <ErrorNotice error={switched} />}
        <UpdateNotice />
        {saveError && <Notice tone="warning">{saveError}</Notice>}
        <ErrorNotice error={signOutError} />
        {children}
      </main>
    </div>
  );
}
