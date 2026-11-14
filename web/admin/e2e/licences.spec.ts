// The Licences page, issuing and copying: a licence issued to a new customer has its keys copied one
// by one, all together, and with the message, and its CSV downloaded; one with no customer is copied
// as it is issued; a refused copy says so; a failed issue shows no copy notice; a key is revealed on
// request; inline edits save on Enter or leaving and cancel on Escape; and the price is filled in from
// the product. Results are checked through the API.

import { readFileSync } from "node:fs";
import { createProduct, issue, licence, unique } from "./data";
import { expect, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

type Page = import("@playwright/test").Page;

// Chrome on Windows stores copied line breaks as CRLF, as the Windows clipboard does
const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText()).then((text) => text.replaceAll("\r\n", "\n"));

/** Waits until no copy button says "Copied", so the next one pressed is the only one that does. */
async function copiedGone(panel: import("@playwright/test").Locator): Promise<void> {
  await expect(panel.getByRole("button", { name: "Copied" })).toHaveCount(0);
}

test("issuing licences to a new customer copies their keys, then copying each, the message and downloading the CSV", async ({ admin, api }) => {
  const product = await createProduct(api);
  const customer = unique("Ana López");
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByRole("combobox", { name: "Customer (optional)" }).fill(customer);
  await panel.getByLabel("New customer's email (optional)").fill(`${customer.replace(/\W/g, "")}@example.com`);
  await panel.getByLabel("How many").fill("2");
  await panel.getByLabel("Expires (optional)").fill("2027-12-31");
  await panel.getByLabel("Notes (optional)").fill("School order 42");
  await panel.getByRole("button", { name: "Issue", exact: true }).click();

  await expect(panel.getByRole("heading", { name: "2 licences issued" })).toBeVisible();
  const keys = await panel.getByRole("list", { name: "New keys" }).locator("code").allTextContents();
  expect(keys).toHaveLength(2);
  keys.forEach((key) => expect(key).toMatch(/^BRN(-[0-9A-Z]{4}){4}$/));
  await expect(panel.getByText("The 2 keys are on the clipboard.")).toBeVisible();
  expect(await clipboard(admin)).toBe(keys.join("\n"));

  await admin.evaluate(() => navigator.clipboard.writeText(""));
  await panel.getByRole("button", { name: "Copy keys" }).click();
  await expect(panel.getByRole("button", { name: "Copied" })).toHaveCount(1);
  expect(await clipboard(admin)).toBe(keys.join("\n"));
  await copiedGone(panel);

  const second = panel.getByRole("list", { name: "New keys" }).getByRole("listitem").nth(1);
  await second.getByRole("button", { name: "Copy" }).click();
  await expect(second.getByRole("button", { name: "Copied" })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Copied" })).toHaveCount(1);
  expect(await clipboard(admin)).toBe(keys[1]);
  await copiedGone(panel);

  await panel.getByRole("button", { name: "Copy message" }).click();
  await expect(panel.getByRole("button", { name: "Copied" })).toHaveCount(1);
  const message = await clipboard(admin);
  expect(message).toContain(`Hello ${customer}`);
  keys.forEach((key) => expect(message).toContain(key));

  const download = admin.waitForEvent("download");
  await panel.getByRole("button", { name: "Download CSV" }).click();
  const csv = readFileSync(await (await download).path(), "utf8").trim().split("\r\n");
  expect(csv[0]).toBe("licence_id,product_code,customer_name,customer_email,key,state,issued_at,expires_at,notes,test,price,currency");
  expect(csv).toHaveLength(3);

  const { data } = await api.get<{ data: { id: string; expires_at: string; internal_notes: string }[] }>(`/licences?product=${product.id}`);
  expect(data).toHaveLength(2);
  data.forEach((row) => expect([row.expires_at, row.internal_notes]).toEqual(["2027-12-31T23:59:59Z", "School order 42"]));
});

test("one licence with no customer is copied as soon as it's issued", async ({ admin, api }) => {
  const product = await createProduct(api);
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByLabel("How many").fill("1");
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("heading", { name: "1 licence issued" })).toBeVisible();
  const key = await panel.getByRole("list", { name: "New keys" }).locator("code").textContent();
  await expect(panel.getByText("The key is on the clipboard.")).toBeVisible();
  expect(await clipboard(admin)).toBe(key);
  await expect(panel.getByRole("button", { name: "Copy key", exact: true })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Copy message" })).toHaveCount(0);
  await expect(panel.getByRole("list", { name: "New keys" }).getByRole("button")).toHaveCount(0);

  // Issue more, then Issue: the new key replaces the old one on the clipboard
  await panel.getByRole("button", { name: "Issue more" }).click();
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("heading", { name: "1 licence issued" })).toBeVisible();
  const next = await panel.getByRole("list", { name: "New keys" }).locator("code").textContent();
  expect(next).not.toBe(key);
  await expect.poll(() => clipboard(admin)).toBe(next);
});

test("a refused copy says so and the keys stay", async ({ admin, api }) => {
  await admin.addInitScript(() => {
    const refuse = () => Promise.reject(new DOMException("denied", "NotAllowedError"));
    navigator.clipboard.write = refuse;
    navigator.clipboard.writeText = refuse;
  });
  const product = await createProduct(api);
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("list", { name: "New keys" }).locator("code")).toHaveText(/^BRN(-[0-9A-Z]{4}){4}$/);
  await expect(panel.getByText("The browser didn't allow copying. Copy it by hand.")).toBeVisible();
  await expect(panel.getByText("The key is on the clipboard.")).toHaveCount(0);
});

test("a failed issue shows no copy notice", async ({ admin, api }) => {
  const errors: Error[] = [];
  admin.on("pageerror", (error) => errors.push(error));
  const product = await createProduct(api);
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await api.call("PATCH", `/products/${product.id}`, { is_active: false });
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("alert")).toBeVisible();
  await expect(panel.getByText(/on the clipboard|Copy it by hand/)).toHaveCount(0);
  await expect(panel.getByRole("heading", { name: "Issue licences" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("a key is revealed on request, and inline edits save on Enter or leaving, and cancel on Escape", async ({ admin, api }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id)).licences[0]!;
  await admin.goto("/licences");
  await admin.getByLabel("Search by key, customer or licence ID").fill(issued.key.toLowerCase().replaceAll("-", " "));
  const row = admin.getByRole("row").filter({ hasText: product.name });
  await expect(row).toHaveCount(1);

  await row.getByRole("button", { name: "Reveal" }).click();
  await expect(row.locator("code.key")).toHaveText(issued.key);

  await row.getByRole("button", { name: "Edit notes" }).click();
  await row.getByLabel("notes").fill("Saved with Enter");
  await row.getByLabel("notes").press("Enter");
  await expect(row.getByRole("button", { name: "Edit notes" })).toHaveText("Saved with Enter");
  await row.getByRole("button", { name: "Edit notes" }).click();
  await row.getByLabel("notes").fill("Saved on leaving");
  await admin.getByRole("heading", { name: "Licences" }).click();
  await expect(row.getByRole("button", { name: "Edit notes" })).toHaveText("Saved on leaving");
  await row.getByRole("button", { name: "Edit notes" }).click();
  await row.getByLabel("notes").fill("Never saved");
  await row.getByLabel("notes").press("Escape");
  await expect(row.getByRole("button", { name: "Edit notes" })).toHaveText("Saved on leaving");

  await row.getByRole("button", { name: "Edit expiry date" }).click();
  await row.getByLabel("expiry date").fill("2028-01-31");
  await row.getByLabel("expiry date").press("Enter");
  await expect(row.getByRole("button", { name: "Edit expiry date" })).toHaveText("31 Jan 2028");

  const saved = await licence(api, issued.id);
  expect([saved.internal_notes, saved.expires_at]).toEqual(["Saved on leaving", "2028-01-31T23:59:59Z"]);
  const preferences = await api.get<{ tables: { licences?: { search?: string } } }>("/preferences");
  expect(preferences.tables.licences?.search, "a key typed into the search is never saved").toBeUndefined();
});

test("issuing fills in the product's price, which can change, and test licences get none", async ({ admin, api }) => {
  const product = await createProduct(api);
  await api.call("PATCH", `/products/${product.id}`, { price: { amount: 1999, currency: "EUR" } });
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  const amount = panel.getByLabel("Price per key", { exact: true });
  await expect(amount).toHaveValue("19.99");
  await expect(panel.getByLabel("Price per key currency")).toHaveValue("EUR");

  await panel.getByLabel(/^Test licences/).check();
  await expect(amount).toBeDisabled();
  await expect(amount).toHaveValue("");
  await panel.getByLabel(/^Test licences/).uncheck();
  await expect(amount).toHaveValue("19.99");

  await amount.fill("15");
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("heading", { name: "1 licence issued" })).toBeVisible();
  const { data } = await api.get<{ data: { price: unknown }[] }>(`/licences?product=${product.id}`);
  expect(data.map((licence) => licence.price)).toEqual([{ amount: 1500, currency: "EUR" }]);
});
