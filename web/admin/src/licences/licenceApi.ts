// The licence calls the pages share: the list address for the current filters, edits, and the
// IDs of every licence matching a filter (for bulk actions on "all that match").

import { api, get, query } from "../shared/api";
import { ApiError } from "../shared/api";
import type { Licence, Page } from "../shared/types";

export const MAX_BATCH = 1000;
export const LICENCE_EXPAND = "product,customer,active_activation";

export type LicenceFilters = { product?: string; customer?: string; state?: string; expiry?: string; q?: string };

/** The fields `/licences/export` accepts as `filter`. */
export function filterBody(filters: LicenceFilters): LicenceFilters {
  const body: LicenceFilters = {};
  for (const name of ["product", "customer", "state", "expiry", "q"] as const) {
    const value = filters[name]?.trim();
    if (value) body[name] = value;
  }
  return body;
}

export function listPath(filters: LicenceFilters, order: string): string {
  return `/licences${query({ ...filterBody(filters), order: order === "expiry" ? "expiry" : undefined, expand: LICENCE_EXPAND })}`;
}

export function updateLicence(id: string, changes: { expires_at?: string | null; internal_notes?: string | null }): Promise<Licence> {
  return api<Licence>("PATCH", `/licences/${id}?expand=${LICENCE_EXPAND}`, changes);
}

/** Every licence ID the filter matches, up to MAX_BATCH; more is an error the admin can act on. */
export async function matchingIds(filters: LicenceFilters): Promise<string[]> {
  const ids: string[] = [];
  let after: string | null = null;
  do {
    const page: Page<Licence> = await get(`/licences${query({ ...filterBody(filters), limit: 100, after })}`);
    ids.push(...page.data.map((licence) => licence.id));
    after = page.next_cursor;
  } while (after && ids.length <= MAX_BATCH);
  if (ids.length > MAX_BATCH) {
    throw new ApiError("invalid_request", `more than ${MAX_BATCH.toLocaleString()} licences match: narrow the filter, or work in parts`, 400);
  }
  return ids;
}

export function csvName(): string {
  return `licences-${new Date().toISOString().slice(0, 10)}.csv`;
}
