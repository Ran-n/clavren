// Helpers shared by the applications specs: a product open for applications, the web form sent for
// a code, and each test's own address for its public requests, so the five-an-hour limit never binds.
// The address counter lives here once, so no two tests in a run share an address.

import type { Page } from "@playwright/test";
import { createProduct } from "./data";
import { expect, type AdminApi } from "./fixtures";

export type Listed = { data: { id: string; name: string; state: string; licence: string | null }[] };

let address = 0;
/** Each test's public requests come from their own address, so the five-an-hour limit never binds. */
export async function ownAddress(page: Page): Promise<void> {
  address += 1;
  const from = `203.0.113.${100 + address}`;
  await page.route("**/api/v1/**", (route) => route.continue({ headers: { ...route.request().headers(), "cf-connecting-ip": from } }));
}

export async function openProduct(api: AdminApi): Promise<{ id: string; code: string; name: string }> {
  const product = await createProduct(api);
  await api.call("PATCH", `/products/${product.id}`, { accepts_applications: true });
  return product;
}

/** Fills in and sends the web form for `code` as `name`; returns the status page's secret. */
export async function apply(page: Page, code: string, name: string, email: string): Promise<string> {
  await page.goto(`/apply?product=${code}`);
  await page.getByLabel("Your name").fill(name);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Message (optional)").fill("For my class");
  await page.getByRole("button", { name: "Send application" }).click();
  await expect(page.getByText("Your application was sent.")).toBeVisible();
  await expect(page).toHaveURL(/\/apply\/status#.+/);
  return decodeURIComponent(new URL(page.url()).hash.slice(1));
}
