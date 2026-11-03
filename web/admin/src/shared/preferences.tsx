// What the dashboard remembers for the admin, on the server so it follows them to any browser:
// theme, last page, and per table its filters, search, order, page size and columns. Changes are
// saved shortly after they happen. A search that looks like a licence key is never saved.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, get } from "./api";
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
};
export type Preferences = { version: 1; theme: Theme; last_page?: string; tables: Partial<Record<TableName, TablePreferences>> };

const SAVE_DELAY_MS = 400;

type PreferencesContext = {
  preferences: Preferences;
  update: (change: (current: Preferences) => Preferences) => void;
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

  useEffect(() => {
    get<Preferences>("/preferences").then(
      (saved) => {
        applyTheme(saved.theme);
        setPreferences({ ...saved, tables: saved.tables ?? {} });
      },
      (error: unknown) => {
        setSaveError(`Your saved settings couldn't be loaded: ${describe(error)}`);
        setPreferences({ version: 1, theme: "system", tables: {} });
      },
    );
    return () => window.clearTimeout(timer.current);
  }, []);

  const update = useCallback((change: (current: Preferences) => Preferences) => {
    setPreferences((current) => {
      if (!current) return current;
      const next = change(current);
      if (next === current) return current; // nothing changed, nothing to save
      applyTheme(next.theme);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        api("PUT", "/preferences", storable(next)).then(
          () => setSaveError(null),
          (error: unknown) => setSaveError(`Your settings couldn't be saved: ${describe(error)}`),
        );
      }, SAVE_DELAY_MS);
      return next;
    });
  }, []);

  const value = useMemo(() => (preferences ? { preferences, update, saveError } : null), [preferences, update, saveError]);
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
