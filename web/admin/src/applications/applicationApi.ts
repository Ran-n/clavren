// The application calls the page shares: the list address for the current filters, and the IDs
// of every application matching a filter (for bulk actions on "all that match").

import { ApiError, get, query } from "../shared/api";
import { formatNumber } from "../shared/format";
import type { Id, Licence, Page, Product, Related } from "../shared/types";

export const MAX_BATCH = 1000;
export const APPLICATION_EXPAND = "product,licence.customer";

export type ApplicationState = "pending" | "approved" | "rejected";

export type Application = {
  id: Id;
  product: Related<Product>;
  name: string;
  email: string;
  message: string | null;
  source: "app" | "web";
  state: ApplicationState;
  created_at: string;
  decided_at: string | null;
  licence: Related<Licence> | null;
  rejection_reason: string | null;
  key_collected_at: string | null;
};

export type ApplicationFilters = { state?: string; product?: string; q?: string };

function filterQuery(filters: ApplicationFilters) {
  return { state: filters.state === "all" ? undefined : filters.state, product: filters.product, q: filters.q?.trim() };
}

export function listPath(filters: ApplicationFilters): string {
  return `/applications${query({ ...filterQuery(filters), expand: APPLICATION_EXPAND })}`;
}

/** Every application ID the filter matches, up to MAX_BATCH; more is an error the admin can act on. */
export async function matchingIds(filters: ApplicationFilters): Promise<string[]> {
  const ids: string[] = [];
  let after: string | null = null;
  do {
    const page: Page<Application> = await get(`/applications${query({ ...filterQuery(filters), limit: 100, after })}`);
    ids.push(...page.data.map((application) => application.id));
    after = page.next_cursor;
  } while (after && ids.length <= MAX_BATCH);
  if (ids.length > MAX_BATCH) {
    throw new ApiError("invalid_request", `more than ${formatNumber(MAX_BATCH)} applications match: narrow the filter, or work in parts`, 400);
  }
  return ids;
}
