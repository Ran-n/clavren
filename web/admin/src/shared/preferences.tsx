// What the dashboard remembers for the admin, on the server so it follows them to any browser:
// theme, last page, and per table its filters, search, order, page size and columns. Changes are
// saved shortly after they happen, and at once before signing out. A search that looks like a
// licence key is never saved. When the saved settings can't be loaded, nothing is saved over them:
// the defaults apply until the page is reloaded. A save sends only what this page changed (the
// theme, the last page, whole tables) on top of the settings saved now, so a tab left open since
// yesterday never puts back what another tab changed meanwhile.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ApiError, api, get, signedInAdmin } from "./api";
import { describe } from "./errors";
import { looksLikeKey } from "./keys";

export type Theme = "system" | "light" | "dark";
export type TableName = "licences" | "customers" | "products" | "activations" | "applications" | "audit" | "backups";
export type TablePreferences = {
  filters?: Record<string, string>;
  search?: string;
  order?: string;
  page_size?: number;
  columns?: string[];
  /** The Issue panel (licences only): never its customer or notes. */
  issue?: IssueDraft;
};
export type IssueDraft = { open: boolean; product?: string; quantity?: number; expires_at?: string };
export type Preferences = { version: 1; theme: Theme; last_page?: string; tables: Partial<Record<TableName, TablePreferences>> };

const SAVE_DELAY_MS = 400;

type PreferencesContext = {
  preferences: Preferences;
  update: (change: (current: Preferences) => Preferences) => void;
  /** Saves a change still waiting for its delay, now. */
  flush: () => Promise<void>;
  saveError: string | null;
};

const Context = createContext<PreferencesContext | null>(null);

/** This browser's copy of the last theme, applied before the saved settings arrive. */
const THEME_KEY = "clavren-theme";

export function applyTheme(theme: Theme): void {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // storage blocked: the theme still applies, from the saved settings, a moment later
  }
}

/** The theme this browser used last, so the page doesn't flash another one while loading. */
export function applyLastTheme(): void {
  let last: string | null = null;
  try {
    last = localStorage.getItem(THEME_KEY);
  } catch {
    return; // storage blocked: the system theme until the saved settings arrive
  }
  if (last === "light" || last === "dark" || last === "system") applyTheme(last);
}

/** The parts of the settings a page changed since its last save. */
type Changed = { theme: boolean; last_page: boolean; tables: Set<TableName> };

const nothingChanged = (): Changed => ({ theme: false, last_page: false, tables: new Set() });

/** What changed from `before` to `after`, added to `changed`; false when nothing did. */
export function noteChanges(before: Preferences, after: Preferences, changed: Changed): boolean {
  let any = false;
  if (before.theme !== after.theme) changed.theme = any = true;
  if (before.last_page !== after.last_page) changed.last_page = any = true;
  const names = new Set([...Object.keys(before.tables), ...Object.keys(after.tables)] as TableName[]);
  for (const name of names) {
    if (JSON.stringify(before.tables[name]) !== JSON.stringify(after.tables[name])) {
      changed.tables.add(name);
      any = true;
    }
  }
  return any;
}

/** `saved` with the parts `changed` taken from `local`. */
export function merged(saved: Preferences, local: Preferences, changed: Changed): Preferences {
  const tables: Preferences["tables"] = { ...(saved.tables ?? {}) };
  for (const name of changed.tables) {
    if (local.tables[name] === undefined) delete tables[name];
    else tables[name] = local.tables[name];
  }
  return {
    ...saved,
    version: 1,
    theme: changed.theme ? local.theme : saved.theme,
    last_page: changed.last_page ? local.last_page : saved.last_page,
    tables,
  };
}

/** What may be stored: everything but values that could be a licence key. */
export function storable(preferences: Preferences): Preferences {
  const tables: Preferences["tables"] = {};
  for (const [name, table] of Object.entries(preferences.tables) as [TableName, TablePreferences][]) {
    const filters = Object.fromEntries(Object.entries(table.filters ?? {}).filter(([, value]) => value && !looksLikeKey(value)));
    const search = table.search && !looksLikeKey(table.search) ? table.search : undefined;
    tables[name] = { ...table, filters, search };
  }
  return { ...preferences, tables };
}

export function PreferencesProvider({ children, loading }: { children: ReactNode; loading: ReactNode }) {
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  // the settings as this page has them, and what it changed since its last save: only that is sent
  const latest = useRef<Preferences | null>(null);
  const changed = useRef<Changed>(nothingChanged());
  // the saved settings couldn't be read: saving the defaults would replace them
  const loadFailed = useRef(false);
  // whose settings these are: after another admin's session took over, they can't be saved
  const owner = useRef(signedInAdmin());

  // saves go one after another: each replaces the whole record, so an older one finishing last
  // would undo a newer one
  const saving = useRef<Promise<void>>(Promise.resolve());

  /** Sends the change waiting for its delay now; rejects when it couldn't be saved. */
  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    const run = saving.current.then(async () => {
      // read when this save's turn comes: a change made while an earlier save was out goes too,
      // and one an earlier save failed to send is still here
      const next = latest.current;
      const parts = changed.current;
      if (loadFailed.current || !next || (!parts.theme && !parts.last_page && parts.tables.size === 0)) return;
      changed.current = nothingChanged();
      try {
        if (signedInAdmin() !== owner.current) {
          throw new ApiError("session_changed", "another admin is signed in now", 403);
        }
        // on top of what is saved now, which another tab may have changed
        const saved = await get<Preferences>("/preferences");
        await api("PUT", "/preferences", storable(merged(saved, next, parts)));
        setSaveError(null);
      } catch (error: unknown) {
        // still unsaved: the next save sends these parts too
        parts.tables.forEach((name) => changed.current.tables.add(name));
        changed.current.theme ||= parts.theme;
        changed.current.last_page ||= parts.last_page;
        setSaveError(`Your settings couldn't be saved: ${describe(error)}`);
        throw error;
      }
    });
    // the next save waits for this one, whatever its outcome
    saving.current = run.catch(() => undefined);
    return run;
  }, []);

  useEffect(() => {
    get<Preferences>("/preferences").then(
      (saved) => {
        applyTheme(saved.theme);
        setPreferences({ ...saved, tables: saved.tables ?? {} });
      },
      (error: unknown) => {
        loadFailed.current = true;
        setSaveError(`Your saved settings couldn't be loaded (${describe(error)}), so changes here aren't saved. Reload the page to try again.`);
        setPreferences({ version: 1, theme: "system", tables: {} });
      },
    );
    // leaving the dashboard: the error can't be shown any more, so it goes to the console
    return () => void flush().catch((error: unknown) => console.error("settings not saved", error));
  }, [flush]);

  const update = useCallback(
    (change: (current: Preferences) => Preferences) => {
      setPreferences((current) => {
        if (!current) return current;
        const next = change(current);
        // nothing changed (a page setting what is already set), nothing to save
        if (next === current || !noteChanges(current, next, changed.current)) return current;
        applyTheme(next.theme);
        latest.current = next;
        if (!loadFailed.current) {
          window.clearTimeout(timer.current);
          // a failure is shown by the provider's notice
          timer.current = window.setTimeout(() => void flush().catch(() => undefined), SAVE_DELAY_MS);
        }
        return next;
      });
    },
    [flush],
  );

  const value = useMemo(
    () => (preferences ? { preferences, update, flush, saveError } : null),
    [preferences, update, flush, saveError],
  );
  if (!value) return <>{loading}</>;
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function usePreferences(): PreferencesContext {
  const context = useContext(Context);
  if (!context) throw new Error("usePreferences outside PreferencesProvider");
  return context;
}

/** One table's remembered settings, and a way to change some of them. */
export function useTablePreferences(name: TableName): [TablePreferences, (change: Partial<TablePreferences>) => void] {
  const { preferences, update } = usePreferences();
  const table = preferences.tables[name] ?? {};
  const change = useCallback(
    (patch: Partial<TablePreferences>) =>
      update((current) => ({ ...current, tables: { ...current.tables, [name]: { ...current.tables[name], ...patch } } })),
    [name, update],
  );
  return [table, change];
}
