// The licence calls the pages share: the list address for the current filters, edits (with the
// customer as the API takes it), and the IDs of every licence matching a filter (for bulk actions
// on "all that match").

import { api, get, query } from "../shared/api";
import { ApiError } from "../shared/api";
import { formatNumber } from "../shared/format";
import type { Licence, Page, Price } from "../shared/types";
import type { CustomerChoice } from "./CustomerPicker";

export const MAX_BATCH = 1000;
/** Added to the list's address after a licence is deleted, so the list says so. */
export const DELETED_NOTICE = "deleted=1";
export const LICENCE_EXPAND = "product,customer,active_activation";

const FILTER_FIELDS = ["product", "customer", "state", "expiry", "q", "test"] as const;
/** Each filter field keeps what it matches; its `not_` twin hides it instead. */
const FILTER_NAMES = [...FILTER_FIELDS, ...FILTER_FIELDS.map((name) => `not_${name}` as const)];

export type LicenceFilters = Partial<Record<(typeof FILTER_NAMES)[number], string>>;

/** The fields `/licences/export` accepts as `filter`. */
export function filterBody(filters: LicenceFilters): LicenceFilters {
  const body: LicenceFilters = {};
  for (const name of FILTER_NAMES) {
    const value = filters[name]?.trim();
    if (value) body[name] = value;
  }
  return body;
}

export function listPath(filters: LicenceFilters, order: string): string {
  return `/licences${query({ ...filterBody(filters), order: order === "expiry" ? "expiry" : undefined, expand: LICENCE_EXPAND })}`;
}

/** A customer as the API takes it: an existing one's ID, a new one, or `null` for none. */
export type CustomerBody = string | { name: string; email?: string } | null;

/** What the picker holds, as the API takes it; `undefined` when nothing is picked or typed. */
export function customerBody(choice: CustomerChoice): Exclude<CustomerBody, null> | undefined {
  if (choice.kind === "existing") return choice.customer.id;
  if (choice.name.trim()) return { name: choice.name.trim(), email: choice.email.trim() || undefined };
  return undefined;
}

export type LicenceChanges = { customer?: CustomerBody; expires_at?: string | null; internal_notes?: string | null; revocation_reason?: string | null; price?: Price | null };

export function updateLicence(id: string, changes: LicenceChanges): Promise<Licence> {
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
    throw new ApiError("invalid_request", `more than ${formatNumber(MAX_BATCH)} licences match: narrow the filter, or work in parts`, 400);
  }
  return ids;
}

export function csvName(): string {
  return `licences-${new Date().toISOString().slice(0, 10)}.csv`;
}
