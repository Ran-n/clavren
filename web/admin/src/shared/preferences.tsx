// What the dashboard remembers for the admin, on the server so it follows them to any browser:
// theme, last page, and per table its filters, search, order, page size and columns. Changes are
// saved shortly after they happen, and at once before signing out. A search that looks like a
// licence key is never saved. When the saved settings can't be loaded, nothing is saved over them:
// the defaults apply until the page is reloaded.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ApiError, api, get, signedInAdmin } from "./api";
import { describe } from "./errors";
import { looksLikeKey } from "./keys";

export type Theme = "system" | "light" | "dark";
export type TableName = "licences" | "customers" | "products" | "activations" | "audit" | "backups";
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

export function applyTheme(theme: Theme): void {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
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
  const pending = useRef<Preferences | null>(null);
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
    const next = pending.current;
    pending.current = null;
    const run = saving.current.then(async () => {
      if (!next) return;
      if (signedInAdmin() !== owner.current) {
        throw new ApiError("session_changed", "another admin is signed in now", 403);
      }
      try {
        await api("PUT", "/preferences", storable(next));
        setSaveError(null);
      } catch (error: unknown) {
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
        if (next === current) return current; // nothing changed, nothing to save
        applyTheme(next.theme);
        if (!loadFailed.current) {
          pending.current = next;
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
