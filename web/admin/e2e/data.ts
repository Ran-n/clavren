// Setting up records through the admin API, so each test starts from what it needs. Names carry
// a per-test suffix: the flows share one database.

import type { AdminApi } from "./fixtures";

export type IssuedLicence = { id: string; key: string; state: string; product: string; customer: string | null; expires_at: string | null; internal_notes: string | null };
type Issued = { licences: IssuedLicence[]; customer: { id: string; name: string } | null; message: string | null };
export type Product = { id: string; code: string; name: string; is_active: boolean; message_template: string };

let counter = 0;
/** Unique within the run: `name-3-k2x`. */
export function unique(name: string): string {
  counter += 1;
  return `${name}-${counter}-${Math.random().toString(36).slice(2, 5)}`;
}

export function createProduct(api: AdminApi, name = unique("Product")): Promise<Product> {
  return api.call<Product>("POST", "/products", { code: name.toLowerCase(), name, message_template: "Hello {customer_name}, your {product_name} key: {key}" });
}

export function issue(api: AdminApi, product: string, extra: Record<string, unknown> = {}): Promise<Issued> {
  return api.call<Issued>("POST", "/licences", { product, ...extra });
}

export function licence(api: AdminApi, id: string): Promise<IssuedLicence & { revocation_reason: string | null }> {
  return api.get(`/licences/${id}`);
}
