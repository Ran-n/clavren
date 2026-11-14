// The shapes of what the dashboard remembers (see preferences.tsx): the theme, each table's
// settings, and the Issue panel's draft.

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
