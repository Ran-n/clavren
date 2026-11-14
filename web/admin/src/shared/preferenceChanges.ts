// What a page changed in the settings, and merging only those parts over the saved ones. Also the
// filter that keeps licence keys out of what is stored. Pure functions: no I/O.

import { looksLikeKey } from "./keys";
import type { Preferences, TableName, TablePreferences } from "./preferenceTypes";

/** The parts of the settings a page changed since its last save. */
export type Changed = { theme: boolean; last_page: boolean; tables: Set<TableName> };

export const nothingChanged = (): Changed => ({ theme: false, last_page: false, tables: new Set() });

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
