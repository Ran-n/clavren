// The Licences page: issuing with a customer typed in place, Copy message, Download CSV, Reveal,
// inline edits (Enter, leaving the field, Escape), and bulk actions. Results are checked through
// the API.

import { readFileSync } from "node:fs";
import { createProduct, issue, licence, unique } from "./data";
import { expect, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

const clipboard = (page: import("@playwright/test").Page) => page.evaluate(() => navigator.clipboard.readText());

test("issuing licences to a new customer, then copying the message and downloading the CSV", async ({ admin, api }) => {
  const product = await createProduct(api);
  const customer = unique("Ana López");
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByLabel("Customer name (optional)").fill(customer);
  await panel.getByLabel("New customer's email (optional)").fill(`${customer.replace(/\W/g, "")}@example.com`);
  await panel.getByLabel("How many").fill("2");
  await panel.getByLabel("Expires (optional)").fill("2027-12-31");
  await panel.getByLabel("Notes (optional)").fill("School order 42");
  await panel.getByRole("button", { name: "Issue", exact: true }).click();

  await expect(panel.getByRole("heading", { name: "2 licences issued" })).toBeVisible();
  const keys = await panel.getByRole("list", { name: "New keys" }).locator("code").allTextContents();
  expect(keys).toHaveLength(2);
  keys.forEach((key) => expect(key).toMatch(/^BRN(-[0-9A-Z]{4}){4}$/));

  await panel.getByRole("button", { name: "Copy message" }).click();
  await expect(panel.getByRole("button", { name: "Copied" })).toBeVisible();
  const message = await clipboard(admin);
  expect(message).toContain(`Hello ${customer}`);
  keys.forEach((key) => expect(message).toContain(key));

  const download = admin.waitForEvent("download");
  await panel.getByRole("button", { name: "Download CSV" }).click();
  const csv = readFileSync(await (await download).path(), "utf8").trim().split("\r\n");
  expect(csv[0]).toBe("licence_id,product_code,customer_name,customer_email,key,state,issued_at,expires_at,notes");
  expect(csv).toHaveLength(3);

  const { data } = await api.get<{ data: { id: string; expires_at: string; internal_notes: string }[] }>(`/licences?product=${product.id}`);
  expect(data).toHaveLength(2);
  data.forEach((row) => expect([row.expires_at, row.internal_notes]).toEqual(["2027-12-31T23:59:59Z", "School order 42"]));
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

test("bulk actions change every selected licence", async ({ admin, api }) => {
  const product = await createProduct(api);
  const customer = unique("Bulk customer");
  const issued = (await issue(api, product.id, { customer: { name: customer }, quantity: 3 })).licences;
  await admin.goto("/licences");
  // The search applies a moment after typing; until then the rows on screen are the unfiltered list.
  const searched = admin.waitForResponse((response) => response.url().includes(`q=${encodeURIComponent(customer).replaceAll("%20", "+")}`));
  await admin.getByLabel("Search by key, customer or licence ID").fill(customer);
  await searched;
  const rows = admin.getByRole("row").filter({ hasText: customer });
  await expect(rows).toHaveCount(3);
  await rows.nth(0).getByLabel("Select this row").check();
  await rows.nth(1).getByLabel("Select this row").check();

  const bar = admin.getByRole("region", { name: "Selected licences" });
  await expect(bar).toContainText("2 licences selected");
  await bar.getByRole("button", { name: "Edit notes" }).click();
  await bar.getByLabel("New notes (empty: none)").fill("Renewed");
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(admin.getByText("2 licences updated.")).toBeVisible();
  await expect(rows.filter({ hasText: "Renewed" })).toHaveCount(2); // the list has reloaded with the change

  await rows.nth(0).getByLabel("Select this row").check();
  await expect(bar).toContainText("1 licence selected");
  await rows.nth(1).getByLabel("Select this row").check();
  await expect(bar).toContainText("2 licences selected");
  await bar.getByRole("button", { name: "Revoke" }).click();
  await bar.getByLabel("Reason (optional)").fill("Refunded");
  await bar.getByRole("group", { name: "Revoke" }).getByRole("button", { name: "Revoke" }).click();
  await expect(admin.getByText("2 licences revoked.")).toBeVisible();

  const states = await Promise.all(issued.map((one) => licence(api, one.id)));
  const changed = states.filter((one) => one.state === "revoked");
  expect(changed).toHaveLength(2);
  changed.forEach((one) => expect([one.internal_notes, one.revocation_reason]).toEqual(["Renewed", "Refunded"]));
  expect(states.filter((one) => one.state === "unused")).toHaveLength(1);
});
