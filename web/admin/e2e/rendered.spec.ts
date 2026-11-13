// A reviewed baseline screenshot of every page, light and dark, at desktop and phone width, from
// a fixed data set on a server whose clock stands still, plus the states that open in place (the
// customer list, the customer editor, the delete warnings, an update, the theme button hovered,
// a new product's test licence, an application being approved), and the public application form
// with each status its page can show, and a product's key page in each of its states.
// Also: nothing makes the page scroll sideways, and disabled actions are dimmed. Keys stay hidden;
// IDs are masked. The update state is a fixed answer, so the server's own version never changes a
// baseline.

import type { Page } from "@playwright/test";
import { Device, expect, test, type AdminApi } from "./fixtures";
import { issue } from "./data";

test.use({ server: "rendered" });

const SIZES = { desktop: { width: 1280, height: 860 }, phone: { width: 390, height: 844 } } as const;
const SCHEMES = ["light", "dark"] as const;

type Seeded = { activeLicence: string; revokedLicence: string; testLicence: string };

/** The data every screenshot shows; made once per server. */
async function seed(api: AdminApi, request: import("@playwright/test").APIRequestContext): Promise<Seeded> {
  const existing = await api.get<{ data: { id: string; state: string; is_test: boolean }[] }>("/licences?limit=100");
  if (existing.data.length > 0) {
    const find = (state: string) => existing.data.find((licence) => licence.state === state && !licence.is_test)?.id as string;
    const testLicence = existing.data.find((licence) => licence.is_test)?.id as string;
    return { activeLicence: find("redeemed"), revokedLicence: find("revoked"), testLicence };
  }
  // Avalio Lite comes with its test licence, used on two computers
  const lite = await api.call<{ id: string; test_licence: { id: string; key: string } }>("POST", "/products", { code: "avalio-lite", name: "Avalio Lite", description: "Gradebook for one teacher", price: { amount: 1999, currency: "EUR" } });
  const pro = await api.call<{ id: string }>("POST", "/products", { code: "avalio-pro", name: "Avalio Pro", description: "For whole schools", test_licence: false });
  await api.call("PATCH", `/products/${pro.id}`, { is_active: false });
  const ana = await issue(api, lite.id, { customer: { name: "Ana López", email: "ana@example.com" }, quantity: 2, expires_at: "2027-12-31T23:59:59Z" });
  await issue(api, lite.id, { customer: { name: "Bruno Díaz" }, internal_notes: "Paid by transfer" });
  await api.call("POST", "/customers", { name: "Carla Gómez", email: "carla@example.com", notes: "Asked about Pro" });
  const spare = await issue(api, lite.id, { quantity: 2 });
  const active = ana.licences[0]!;
  await new Device("ANA-LAPTOP").activate(request, "avalio-lite", active.key, "203.0.113.50");
  const revoked = spare.licences[0]!;
  await api.call("POST", `/licences/${revoked.id}/revoke`, { reason: "Refunded" });
  for (const computer of ["DEV-PC", "TEST-VM"]) {
    await new Device(computer).activate(request, "avalio-lite", lite.test_licence.key, "203.0.113.60");
  }
  await api.call("PATCH", `/products/${lite.id}`, { accepts_applications: true });
  const applied: Record<string, string> = {};
  for (const [name, message, source] of [
    ["Dana Ruiz", "I teach maths at IES Rosalía and would like to try it with two groups.", "web"],
    ["Eva Martín", null, "app"],
    ["Fede Ortiz", "For my own use", "web"],
  ] as const) {
    const response = await request.post("/api/v1/applications", {
      data: { product: "avalio-lite", name, email: `${name.split(" ")[0]!.toLowerCase()}@example.com`, message: message ?? undefined, source },
      headers: { "cf-connecting-ip": "203.0.113.70" },
    });
    applied[name] = ((await response.json()) as { application: string }).application;
  }
  await api.call("POST", `/applications/${applied["Eva Martín"]}/approve`, { customer: { name: "Eva Martín", email: "eva@example.com" } });
  await api.call("POST", `/applications/${applied["Fede Ortiz"]}/reject`, { reason: "Only for schools" });
  return { activeLicence: active.id, revokedLicence: revoked.id, testLicence: lite.test_licence.id };
}

/** What creating a product answers in the capture of its test licence: fixed, so no key changes a baseline. */
const CREATED_PRODUCT = {
  id: "00000000-0000-7000-8000-000000000001",
  code: "avalio-lab",
  name: "Avalio Lab",
  description: null,
  key_instructions: null,
  offline_days: 30,
  price: null,
  message_template: "",
  is_active: true,
  created_at: "2027-03-01T09:00:00Z",
  updated_at: "2027-03-01T09:00:00Z",
  test_licence: { id: "00000000-0000-7000-8000-000000000002", key: "BRN-TEST-0000-0000-0000" },
};

/** What issuing three keys answers in the capture of the issued keys: fixed, so no key changes a baseline. */
const ISSUED_THREE = {
  licences: ["BRN-7KPX-4M9D-QW2R-8NFC", "BRN-3HTV-9C2M-XK4P-6WQD", "BRN-5RNE-8F7T-JM3K-2YBH"].map((key, index) => ({
    id: `00000000-0000-7000-8000-00000000001${index}`,
    key,
    is_test: false,
  })),
  customer: null,
  message: null,
};

/** `GET /updates` as every capture sees it, unless a test answers otherwise. */
const NO_UPDATE = {
  running_version: "1.0.0",
  checking: "on",
  last_check: { checked_at: "2027-03-01T06:12:00Z", error: null },
  available: null,
  backup_drive_mounted: true,
  update: null,
  updater_installed: true,
  can_install: false,
};
const UPDATE_AVAILABLE = {
  ...NO_UPDATE,
  available: { version: "1.1.0", release_url: "https://github.com/Ran-n/clavren/releases/tag/v1.1.0", published_at: "2027-02-28T10:00:00Z", installable: true },
  backup_drive_mounted: false,
  can_install: true,
};
const UPDATE_RUNNING = {
  ...UPDATE_AVAILABLE,
  update: { version: "1.1.0", state: "running", stage: "backing_up", message: null, started_at: "2027-03-01T09:29:00Z", finished_at: null },
  can_install: false,
};

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

async function answerUpdates(page: Page, body: object): Promise<void> {
  await page.unroute("**/api/admin/updates");
  await page.route("**/api/admin/updates", (route) => route.fulfill({ json: body }));
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
  test.beforeEach(async ({ api, request, page }) => {
    seeded = await seed(api, request);
    await answerUpdates(page, NO_UPDATE);
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
      // the panel is remembered open after the first size and scheme
      const open = page.getByRole("button", { name: "Issue licences" });
      if (await open.isVisible()) await open.click();
      await page.getByRole("combobox", { name: "Customer (optional)" }).focus();
      await expect(page.getByRole("option", { name: /Carla Gómez/ })).toBeVisible();
    }));
  test("three keys just issued, on the clipboard, each with Copy", async ({ admin }) => {
    await admin.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await admin.route("**/api/admin/licences", (route) =>
      route.request().method() === "POST" ? route.fulfill({ status: 201, json: ISSUED_THREE }) : route.fallback(),
    );
    await capture(admin, "issued-keys", "/licences", undefined, async (page) => {
      const open = page.getByRole("button", { name: "Issue licences" });
      if (await open.isVisible()) await open.click();
      await page.getByRole("region", { name: "Issue licences" }).getByRole("button", { name: "Issue", exact: true }).click();
      await expect(page.getByText("The 3 keys are on the clipboard.")).toBeVisible();
    });
  });
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
  test("a product's offline days being lowered", async ({ admin }) =>
    capture(admin, "product-offline", "/products", undefined, async (page) => {
      const row = page.getByRole("row").filter({ hasText: "Avalio Lite" });
      await row.getByRole("button", { name: "Edit days offline" }).click();
      await row.getByLabel("days offline").fill("7");
      await row.getByLabel("days offline").press("Enter");
      await expect(page.getByRole("group", { name: "Limit" })).toBeVisible();
    }));
  test("deleting a product with licences", async ({ admin }) =>
    capture(admin, "product-delete", "/products", undefined, async (page) => {
      await page.getByRole("row").filter({ hasText: "Avalio Lite" }).getByRole("button", { name: "Delete" }).click();
      await expect(page.getByRole("group", { name: "Delete" })).toContainText("Type avalio-lite to confirm");
    }));
  test("a test licence on two computers, being reset", async ({ admin }) =>
    capture(admin, "licence-test-reset", `/licences/${seeded.testLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Reset" }).click();
      await expect(page.getByRole("group", { name: "Reset" })).toContainText("Every computer using it locks");
    }));
  test("a new product with its test licence", async ({ admin }) => {
    // only the creation is answered here: the list is the real one, unchanged
    await admin.route("**/api/admin/products", (route) =>
      route.request().method() === "POST" ? route.fulfill({ status: 201, json: CREATED_PRODUCT }) : route.fallback(),
    );
    await capture(admin, "product-test-licence", "/products", undefined, async (page) => {
      await page.getByRole("button", { name: "New product" }).click();
      await page.getByLabel("New product's name").fill(CREATED_PRODUCT.name);
      await page.getByLabel("New product's name").press("Enter");
      await expect(page.getByText("Avalio Lab was created with a test licence")).toBeVisible();
    });
  });
  test("applications", async ({ admin }) => capture(admin, "applications", "/applications"));
  test("an application being approved", async ({ admin }) =>
    capture(admin, "application-approving", "/applications", () => [], async (page) => {
      await page.getByRole("row").filter({ hasText: "Dana Ruiz" }).getByRole("button", { name: "Approve" }).click();
      await expect(page.getByRole("combobox", { name: /^Customer/ })).toHaveValue("Dana Ruiz");
    }));
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
  test("the theme button hovered", async ({ admin }) =>
    capture(admin, "theme-button-hover", "/", undefined, async (page) => {
      await page.getByRole("button", { name: /^Theme:/ }).hover();
    }));
  test("an update being confirmed without a backup drive", async ({ admin }) => {
    await answerUpdates(admin, UPDATE_AVAILABLE);
    await capture(admin, "update-confirm", "/", undefined, async (page) => {
      await page.getByRole("button", { name: "Update to 1.1.0" }).click();
      await expect(page.getByRole("group", { name: "Confirm" })).toContainText("Update without a backup");
    });
    const confirm = admin.getByRole("group", { name: "Confirm" }).getByRole("button", { name: "Confirm" });
    await expect(confirm).toBeDisabled();
    expect(Number(await confirm.evaluate((element) => getComputedStyle(element).opacity))).toBeLessThan(0.6);
  });
  test("an update running", async ({ admin }) => {
    await answerUpdates(admin, UPDATE_RUNNING);
    await capture(admin, "update-running", "/licences", undefined, async (page) => {
      await expect(page.getByText("Updating to 1.1.0…")).toBeVisible();
    });
  });
});
