// A reviewed baseline screenshot of every page, light and dark, at desktop and phone width, from
// a fixed data set on a server whose clock stands still, plus the states that open in place (the
// customer list, the customer editor, the delete warnings). Also: nothing makes the page scroll
// sideways, and disabled actions are dimmed. Keys stay hidden; IDs are masked.

import type { Page } from "@playwright/test";
import { Device, expect, test, type AdminApi } from "./fixtures";
import { issue } from "./data";

test.use({ server: "rendered" });

const SIZES = { desktop: { width: 1280, height: 860 }, phone: { width: 390, height: 844 } } as const;
const SCHEMES = ["light", "dark"] as const;

type Seeded = { activeLicence: string; revokedLicence: string };

/** The data every screenshot shows; made once per server. */
async function seed(api: AdminApi, request: import("@playwright/test").APIRequestContext): Promise<Seeded> {
  const existing = await api.get<{ data: { id: string; state: string }[] }>("/licences?limit=100");
  if (existing.data.length > 0) {
    const find = (state: string) => existing.data.find((licence) => licence.state === state)?.id as string;
    return { activeLicence: find("redeemed"), revokedLicence: find("revoked") };
  }
  const lite = await api.call<{ id: string }>("POST", "/products", { code: "avalio-lite", name: "Avalio Lite", description: "Gradebook for one teacher" });
  const pro = await api.call<{ id: string }>("POST", "/products", { code: "avalio-pro", name: "Avalio Pro", description: "For whole schools" });
  await api.call("PATCH", `/products/${pro.id}`, { is_active: false });
  const ana = await issue(api, lite.id, { customer: { name: "Ana López", email: "ana@example.com" }, quantity: 2, expires_at: "2027-12-31T23:59:59Z" });
  await issue(api, lite.id, { customer: { name: "Bruno Díaz" }, internal_notes: "Paid by transfer" });
  await api.call("POST", "/customers", { name: "Carla Gómez", email: "carla@example.com", notes: "Asked about Pro" });
  const spare = await issue(api, lite.id, { quantity: 2 });
  const active = ana.licences[0]!;
  await new Device("ANA-LAPTOP").activate(request, "avalio-lite", active.key, "203.0.113.50");
  const revoked = spare.licences[0]!;
  await api.call("POST", `/licences/${revoked.id}/revoke`, { reason: "Refunded" });
  return { activeLicence: active.id, revokedLicence: revoked.id };
}

type Locators = (page: Page) => import("@playwright/test").Locator[];
const masked: (selector: string) => Locators = (selector) => (page) => [page.locator(selector)];
const LICENCE_ID = masked("code.licence-id");

/** `path` at every size and scheme; `prepare` opens a state on the page before the capture. */
async function capture(page: Page, name: string, path: string, masks: Locators = () => [], prepare?: (page: Page) => Promise<void>) {
  for (const [size, viewport] of Object.entries(SIZES)) {
    for (const scheme of SCHEMES) {
      await page.setViewportSize(viewport);
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto(path);
      await expect(page.locator("[aria-busy='true'], .loading")).toHaveCount(0);
      if (prepare) await prepare(page);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, `${name} scrolls sideways at ${size} width`).toBeLessThanOrEqual(0);
      await expect(page).toHaveScreenshot(`${name}-${size}-${scheme}.png`, { fullPage: true, mask: masks(page) });
    }
  }
}

test.describe("every page, light and dark, desktop and phone", () => {
  let seeded: Seeded;
  test.beforeEach(async ({ api, request }) => {
    seeded = await seed(api, request);
  });

  test("sign-in", async ({ page }) => capture(page, "sign-in", "/"));
  test("overview", async ({ admin }) => capture(admin, "overview", "/"));
  test("licences", async ({ admin }) => capture(admin, "licences", "/licences"));
  test("licence in use", async ({ admin }) => capture(admin, "licence-in-use", `/licences/${seeded.activeLicence}`, LICENCE_ID));
  test("revoked licence, actions dimmed", async ({ admin }) => {
    await capture(admin, "licence-revoked", `/licences/${seeded.revokedLicence}`, LICENCE_ID);
    for (const name of ["Reset", "Revoke"]) {
      const button = admin.getByRole("button", { name });
      await expect(button).toBeDisabled();
      expect(Number(await button.evaluate((element) => getComputedStyle(element).opacity))).toBeLessThan(0.6);
    }
  });
  test("issuing, with the list of every customer open", async ({ admin }) =>
    capture(admin, "issue-customers", "/licences", undefined, async (page) => {
      await page.getByRole("button", { name: "Issue licences" }).click();
      await page.getByRole("combobox", { name: "Customer (optional)" }).focus();
      await expect(page.getByRole("option", { name: /Carla Gómez/ })).toBeVisible();
    }));
  test("a licence's customer being changed", async ({ admin }) =>
    capture(admin, "licence-customer-editor", `/licences/${seeded.activeLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Change" }).click();
      await expect(page.getByRole("group", { name: "Customer" })).toBeVisible();
    }));
  test("deleting a licence in use", async ({ admin }) =>
    capture(admin, "licence-delete", `/licences/${seeded.activeLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Delete" }).click();
      await expect(page.getByRole("group", { name: "Delete" })).toContainText("locks at its next check");
    }));
  test("deleting a product with licences", async ({ admin }) =>
    capture(admin, "product-delete", "/products", undefined, async (page) => {
      await page.getByRole("row").filter({ hasText: "Avalio Lite" }).getByRole("button", { name: "Delete" }).click();
      await expect(page.getByRole("group", { name: "Delete" })).toContainText("Type avalio-lite to confirm");
    }));
  test("customers", async ({ admin }) => capture(admin, "customers", "/customers"));
  test("products", async ({ admin }) => capture(admin, "products", "/products"));
  test("activations", async ({ admin }) => capture(admin, "activations", "/activations"));
  test("audit log", async ({ admin }) => capture(admin, "audit", "/audit", masked('td[data-label="Record"]')));
  test("account", async ({ admin }) => capture(admin, "account", "/account"));
});
