// Reviewed baseline screenshots of the pages, light and dark, at desktop and phone width: each page from
// a fixed data set on a server whose clock stands still, the application form with each status its page
// can show, and a product's key page in each of its states. The revoked licence's actions show dimmed.
// Keys stay hidden and IDs are masked. The update check is a fixed answer, so the server's own version
// never changes a baseline. The states that open in place are in rendered-states.spec.ts.

import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { arrange, capture, LICENCE_ID, masked, type Seeded } from "./rendered-helpers";

test.use({ server: "rendered" });

/** What the status page shows, fixed: the real secret of an application isn't kept between runs. */
function applicationStatus(status: string, extra: object = {}) {
  return {
    application: "00000000-0000-7000-8000-000000000003",
    product: "avalio-lite",
    status,
    created_at: "2027-03-01T08:40:00Z",
    decided_at: status === "pending" ? null : "2027-03-01T09:10:00Z",
    reason: null,
    key: null,
    licence_expires_at: null,
    server_time: "2027-03-01T09:30:00Z",
    ...extra,
  };
}

/** A product's key page as `GET /api/v1/applications/products/avalio-lite` answers, fixed. */
function keyPage(extra: object = {}) {
  return {
    code: "avalio-lite",
    name: "Avalio Lite",
    description: "Gradebook for one teacher",
    key_instructions: "Schools get one key per teacher for free: write to licences@example.com from your school address.\nEveryone else can buy one at https://example.com/avalio.",
    accepts_applications: true,
    server_time: "2027-03-01T09:30:00Z",
    ...extra,
  };
}

async function answerKeyPage(page: Page, status: number, body: object): Promise<void> {
  await page.route("**/api/v1/applications/products/*", (route) => route.fulfill({ status, json: body }));
}

async function answerStatus(page: Page, body: object): Promise<void> {
  await page.route("**/api/v1/applications/status", (route) => route.fulfill({ json: body }));
}

test.describe("every page, light and dark, desktop and phone", () => {
  let seeded: Seeded;
  test.beforeEach(async ({ api, request, page }) => {
    seeded = await arrange(api, request, page);
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
  test("applications", async ({ admin }) => capture(admin, "applications", "/applications"));
  test("the application form", async ({ page }) => capture(page, "apply", "/apply"));
  test("a product's key page, taking applications", async ({ page }) => {
    await answerKeyPage(page, 200, keyPage());
    await capture(page, "apply-key-open", "/apply?product=avalio-lite");
  });
  test("a product's key page, closed, with instructions", async ({ page }) => {
    await answerKeyPage(page, 200, keyPage({ accepts_applications: false }));
    await capture(page, "apply-key-closed", "/apply?product=avalio-lite");
  });
  test("a product's key page, for a product that isn't available", async ({ page }) => {
    await answerKeyPage(page, 404, { error: "product_unavailable", message: "this product isn't available", server_time: "2027-03-01T09:30:00Z" });
    await capture(page, "apply-key-unavailable", "/apply?product=avalio-pro");
  });
  test("an application waiting", async ({ page }) => {
    await answerStatus(page, applicationStatus("pending"));
    await capture(page, "apply-pending", "/apply/status#secret");
  });
  test("an application approved, with its key", async ({ page }) => {
    await answerStatus(page, applicationStatus("approved", { key: "BRN-7KPX-4M9D-QW2R-8NFC", licence_expires_at: "2027-12-31T23:59:59Z" }));
    await capture(page, "apply-approved", "/apply/status#secret");
  });
  test("an application rejected, with its reason", async ({ page }) => {
    await answerStatus(page, applicationStatus("rejected", { reason: "Only for schools" }));
    await capture(page, "apply-rejected", "/apply/status#secret");
  });
  test("customers", async ({ admin }) => capture(admin, "customers", "/customers"));
  test("products", async ({ admin }) => capture(admin, "products", "/products"));
  test("activations", async ({ admin }) => capture(admin, "activations", "/activations"));
  test("audit log", async ({ admin }) => capture(admin, "audit", "/audit", masked('td[data-label="Record"]')));
  test("account", async ({ admin }) => capture(admin, "account", "/account"));
});
