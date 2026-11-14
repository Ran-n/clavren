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
import { merged, noteChanges, nothingChanged, storable, type Changed } from "./preferenceChanges";
import type { Preferences, TableName, TablePreferences } from "./preferenceTypes";
import { applyTheme } from "./themeStorage";

const SAVE_DELAY_MS = 400;

type PreferencesContext = {
  preferences: Preferences;
  update: (change: (current: Preferences) => Preferences) => void;
  /** Saves a change still waiting for its delay, now. */
  flush: () => Promise<void>;
  saveError: string | null;
};

const Context = createContext<PreferencesContext | null>(null);

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
