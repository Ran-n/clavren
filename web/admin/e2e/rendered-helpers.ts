// Helpers for the rendered screenshot specs (rendered.spec.ts and rendered-states.spec.ts): the fixed
// data set they show, the update check answered as "no update" unless a test answers otherwise, and
// the capture of a page at every size and colour scheme, which also checks it doesn't scroll sideways.

import type { Page } from "@playwright/test";
import { issue } from "./data";
import { Device, expect, type AdminApi } from "./fixtures";

const SIZES = { desktop: { width: 1280, height: 860 }, phone: { width: 390, height: 844 } } as const;
const SCHEMES = ["light", "dark"] as const;

export type Seeded = { activeLicence: string; revokedLicence: string; testLicence: string };

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

/** `GET /updates` as every capture sees it, unless a test answers otherwise. */
export const NO_UPDATE = {
  running_version: "1.0.0",
  checking: "on",
  last_check: { checked_at: "2027-03-01T06:12:00Z", error: null },
  available: null,
  backup_drive_mounted: true,
  update: null,
  updater_installed: true,
  can_install: false,
};

export async function answerUpdates(page: Page, body: object): Promise<void> {
  await page.unroute("**/api/admin/updates");
  await page.route("**/api/admin/updates", (route) => route.fulfill({ json: body }));
}

type Locators = (page: Page) => import("@playwright/test").Locator[];
export const masked: (selector: string) => Locators = (selector) => (page) => [page.locator(selector)];
export const LICENCE_ID = masked("code.licence-id");

/** `path` at every size and scheme; `prepare` opens a state on the page before the capture. */
export async function capture(page: Page, name: string, path: string, masks: Locators = () => [], prepare?: (page: Page) => Promise<void>) {
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

export async function arrange(api: AdminApi, request: import("@playwright/test").APIRequestContext, page: Page): Promise<Seeded> {
  const seeded = await seed(api, request);
  await answerUpdates(page, NO_UPDATE);
  return seeded;
}
