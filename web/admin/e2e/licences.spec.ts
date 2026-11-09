// The Licences page: issuing with a customer picked from the list of every customer or typed in
// place, Copy message, Download CSV, Reveal, inline edits (Enter, leaving the field, Escape),
// filters that hide what they name, and bulk actions. Results are checked through the API.

import { readFileSync } from "node:fs";
import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, newSession, signedInPage, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

type Page = import("@playwright/test").Page;

const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());
/** The toolbar's Product filter (the column picker has a "Product" checkbox too). */
const productFilter = (page: Page) => page.getByRole("combobox", { name: /^Product/ });

test("issuing licences to a new customer, then copying the message and downloading the CSV", async ({ admin, api }) => {
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

test("filters hide what they name, in the list, in bulk and in the CSV", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const [kept, hidden] = [unique("Kept customer"), unique("Hidden customer")];
  const licences = (await issue(api, product.id, { customer: { name: kept }, quantity: 12 })).licences;
  await issue(api, product.id, { customer: { name: hidden } });
  await api.call("POST", `/licences/${licences[0]!.id}/revoke`, {});
  await new Device("EXCEPT-PC").activate(request, product.code, licences[1]!.key, "203.0.113.31");
  await admin.goto("/licences");
  await admin.getByLabel("Rows").selectOption("25");
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(13);

  await admin.getByLabel("State").selectOption({ label: "Except revoked" });
  await expect(rows).toHaveCount(12);
  await expect(rows.filter({ hasText: "Revoked" })).toHaveCount(0);
  await expect(rows.filter({ hasText: "In use" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "Unused" })).toHaveCount(11);

  await admin.getByLabel("Hide key, customer or licence ID").fill(hidden);
  await expect(rows.filter({ hasText: hidden })).toHaveCount(0);
  await expect(rows).toHaveCount(11);

  await admin.getByLabel("Rows").selectOption("10");
  await expect(rows).toHaveCount(10);
  await admin.getByLabel("Select every row on this page").check();
  await admin.getByRole("button", { name: "Select all that match the filter" }).click();
  const download = admin.waitForEvent("download");
  await admin.getByRole("region", { name: "Selected licences" }).getByRole("button", { name: "Export CSV" }).click();
  const csv = readFileSync(await (await download).path(), "utf8").trim().split("\r\n").slice(1);
  expect(csv).toHaveLength(11);
  csv.forEach((line) => expect(line).not.toMatch(/,revoked,|Hidden customer/));
});

test("a customer's link can hide that customer instead", async ({ admin, api }) => {
  const product = await createProduct(api);
  const customer = unique("Linked customer");
  const issued = await issue(api, product.id, { customer: { name: customer } });
  await issue(api, product.id, { quantity: 2 });
  await admin.goto(`/licences?customer=${issued.customer!.id}`);
  await expect(admin).toHaveURL(/\/licences$/);
  await expect(admin.getByText("Showing one customer's licences.")).toBeVisible();
  await admin.getByRole("button", { name: "Hide this customer instead" }).click();
  await expect(admin.getByText("Hiding one customer's licences.")).toBeVisible();
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: customer })).toHaveCount(0);
  await admin.getByRole("button", { name: "Show every customer" }).click();
  await expect(rows).toHaveCount(3);

  await admin.goto(`/licences?not_customer=${issued.customer!.id}`);
  await expect(admin.getByText("Hiding one customer's licences.")).toBeVisible();
  await expect(admin).toHaveURL(/\/licences$/);
});

test("the customer field lists every customer A to Z, narrows as you type, and picks by mouse or keyboard", async ({ admin, api }) => {
  const prefix = unique("Zz Scroll");
  const numbered = Array.from({ length: 60 }, (_, index) => `${prefix} ${String(index + 1).padStart(2, "0")}`);
  for (const name of numbered) await api.call("POST", "/customers", { name });
  const ana = unique("Ana Picker");
  const email = `${ana.replace(/\W/g, "").toLowerCase()}@example.com`;
  await api.call("POST", "/customers", { name: ana, email });
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  const field = panel.getByRole("combobox", { name: "Customer (optional)" });
  const list = panel.getByRole("listbox", { name: "Customer (optional)" });
  const options = list.getByRole("option");

  await field.focus();
  await expect(options.first()).toBeVisible();
  await expect(field).toHaveAttribute("aria-expanded", "true");
  const last = options.filter({ hasText: numbered[59] });
  await expect
    .poll(async () => {
      await panel.locator(".combobox__popup").evaluate((popup) => popup.scrollTo(0, popup.scrollHeight));
      return last.count();
    })
    .toBe(1);
  const shown = (await options.allTextContents()).filter((name) => name.startsWith(prefix));
  expect(shown, "A to Z, every one").toEqual(numbered);

  await field.fill(ana.slice(0, 10)); // "Ana Picker"
  await expect(options.filter({ hasText: prefix })).toHaveCount(0);
  await expect(options.filter({ hasText: ana })).toHaveCount(1);
  await expect(list.getByRole("option", { name: /^New customer/ })).toBeVisible();

  await field.fill(ana);
  await expect(options.first()).toContainText(ana);
  await expect(list.getByRole("option", { name: /^New customer/ }), "an existing name isn't offered as new").toHaveCount(0);
  await field.press("ArrowDown");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  await field.press("Enter");
  await expect(panel.getByText(`${ana} · ${email}`)).toBeVisible();
  await expect(panel.getByLabel("New customer's email (optional)")).toHaveCount(0);

  await panel.getByRole("button", { name: "Change" }).click();
  const unknown = unique("Nobody Yet");
  await field.fill(unknown);
  await list.getByRole("option", { name: `New customer: "${unknown}"` }).click();
  await expect(list).toBeHidden();
  await expect(panel.getByLabel("New customer's email (optional)")).toBeVisible();

  await field.fill("");
  await field.click();
  await expect(list).toBeVisible();
  await options.filter({ hasText: numbered[0] }).click();
  await expect(panel.getByText(numbered[0]!)).toBeVisible();
  await panel.getByRole("button", { name: "Change" }).click();
  await field.focus();
  await expect(list).toBeVisible();
  await field.press("Escape");
  await expect(list).toBeHidden();
  await expect(panel, "the first Escape only closes the list").toBeVisible();
});

test("a customer list that fails to load says so, and Retry recovers", async ({ admin }) => {
  const failing = "**/api/admin/customers?*";
  await admin.route(failing, (route) => route.fulfill({ status: 500, contentType: "application/json", body: '{"error":"internal_error","message":""}' }));
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  const field = panel.getByRole("combobox", { name: "Customer (optional)" });
  await field.focus();
  const status = panel.getByRole("status").filter({ hasText: "Couldn't load customers." });
  await expect(status).toBeVisible();

  await admin.unroute(failing);
  await status.getByRole("button", { name: "Retry" }).click();
  await expect(panel.getByRole("listbox", { name: "Customer (optional)" }).getByRole("option").first()).toBeVisible();
  await expect(status).toHaveCount(0);
});

test("a row's customer is set in place, and the bulk bar sets customers and reasons all or nothing", async ({ admin, api }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id, { quantity: 3 })).licences;
  const revoked = issued[0]!;
  await api.call("POST", `/licences/${revoked.id}/revoke`, {});
  await admin.goto("/licences");
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(3);

  const buyer = unique("Row buyer");
  const row = rows.filter({ hasText: "Unused" }).first();
  await row.hover();
  await row.getByRole("button", { name: "Edit customer" }).click();
  const editor = admin.getByRole("group", { name: "Customer" });
  await editor.getByRole("combobox", { name: "Customer (optional)" }).fill(buyer);
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(rows.filter({ hasText: buyer })).toHaveCount(1);
  await expect(admin, "without leaving the list").toHaveURL(/\/licences$/);

  const unused = rows.filter({ hasText: "Unused" });
  await unused.nth(0).getByLabel("Select this row").check();
  await unused.nth(1).getByLabel("Select this row").check();
  const bar = admin.getByRole("region", { name: "Selected licences" });
  await bar.getByRole("button", { name: "Set customer" }).click();
  const bulkBuyer = unique("Bulk buyer");
  await bar.getByRole("combobox", { name: "Customer (optional)" }).fill(bulkBuyer);
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(admin.getByText("2 licences updated.")).toBeVisible();
  await expect(rows.filter({ hasText: bulkBuyer })).toHaveCount(2);

  await rows.filter({ hasText: "Revoked" }).getByLabel("Select this row").check();
  await rows.filter({ hasText: "Unused" }).first().getByLabel("Select this row").check();
  await bar.getByRole("button", { name: "Edit reason" }).click();
  await bar.getByLabel("New reason (empty: none)").fill("Refunded");
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(bar.getByRole("alert")).toContainText("Only a revoked licence has a revocation reason.");
  expect((await licence(api, revoked.id)).revocation_reason, "nothing changed").toBeNull();
});

test("bulk delete warns with the server's counts, for a selection and for everything that matches", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id, { quantity: 12 })).licences;
  await new Device("BULK-DELETE-PC").activate(request, product.code, issued[0]!.key, "203.0.113.42");
  await admin.goto("/licences");
  await admin.getByLabel("Rows").selectOption("10");
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(10);
  const bar = admin.getByRole("region", { name: "Selected licences" });

  await admin.getByLabel("Rows").selectOption("25");
  await expect(rows).toHaveCount(12);
  await rows.filter({ hasText: "In use" }).getByLabel("Select this row").check();
  await rows.filter({ hasText: "Unused" }).first().getByLabel("Select this row").check();
  await bar.getByRole("button", { name: "Delete" }).click();
  const strip = bar.getByRole("group", { name: "Delete" });
  await expect(strip).toContainText("Delete 2 licences? 1 is in use: that computer locks at its next check.");
  await strip.getByRole("button", { name: "Cancel" }).click();
  await bar.getByRole("button", { name: "Clear selection" }).click();

  await admin.getByLabel("Rows").selectOption("10");
  await admin.getByLabel("Select every row on this page").check();
  await admin.getByRole("button", { name: "Select all that match the filter" }).click();
  await bar.getByRole("button", { name: "Delete" }).click();
  await expect(strip).toContainText("Delete 12 licences? 1 is in use");
  await strip.getByRole("button", { name: "Delete" }).click();
  await expect(admin.getByText("12 licences deleted.")).toBeVisible();
  await expect(admin.getByText("No licence matches.")).toBeVisible();
  expect((await api.get<{ data: unknown[] }>(`/licences?product=${product.id}`)).data).toHaveLength(0);
});

// Regression: Enter in the customer field, with no suggestion highlighted, submitted the form:
// typing a new customer's name and pressing Enter issued licences at once.
test("Enter in the customer field never issues", async ({ admin, api }) => {
  const product = await createProduct(api);
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByRole("combobox", { name: "Customer (optional)" }).fill(unique("Typed then Enter"));
  await panel.getByRole("combobox", { name: "Customer (optional)" }).press("Enter");
  await admin.waitForTimeout(300);
  await expect(panel.getByRole("button", { name: "Issue", exact: true })).toBeVisible();
  expect((await api.get<{ data: unknown[] }>(`/licences?product=${product.id}`)).data).toHaveLength(0);
});

// Regression: unticking one row while "all that match" was selected unticked every other row.
test("unticking a row of all that match keeps the rest of the page ticked", async ({ admin, api }) => {
  const product = await createProduct(api);
  await issue(api, product.id, { quantity: 12 });
  await admin.goto("/licences");
  await productFilter(admin).selectOption({ label: product.name });
  await admin.getByLabel("Rows").selectOption("10");
  const boxes = admin.getByRole("table", { name: "Licences" }).getByLabel("Select this row");
  await expect(boxes).toHaveCount(10);
  await admin.getByLabel("Select every row on this page").check();
  await admin.getByRole("button", { name: "Select all that match the filter" }).click();
  await admin.getByRole("navigation", { name: "Table pages" }).getByRole("button", { name: "Next" }).click();
  await expect(boxes).toHaveCount(2);
  await expect(boxes.first()).toBeChecked();
  await boxes.first().uncheck();
  await expect(boxes.first()).not.toBeChecked();
  await expect(boxes.nth(1)).toBeChecked();
  await admin.getByRole("navigation", { name: "Table pages" }).getByRole("button", { name: "Previous" }).click();
  await expect(boxes).toHaveCount(10);
  for (let index = 0; index < 10; index += 1) await expect(boxes.nth(index)).toBeChecked();

  // the bulk action takes every matching licence but the one left out
  const bar = admin.getByRole("region", { name: "Selected licences" });
  await expect(bar).toContainText("except 1 unticked licence");
  const download = admin.waitForEvent("download");
  await bar.getByRole("button", { name: "Export CSV" }).click();
  const csv = readFileSync(await (await download).path(), "utf8").trim().split("\r\n").slice(1);
  expect(csv).toHaveLength(11);
});

// The retried request carries its body again: a bulk change after signing in again in another tab.
test("a bulk delete after signing in again in another tab deletes every selected licence", async ({ admin, api, request, credentials, baseURL }) => {
  const product = await createProduct(api);
  await issue(api, product.id, { quantity: 2 });
  await admin.goto("/licences");
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(2);
  await admin.waitForTimeout(600); // the filter is saved by now: Delete is the next change
  const { username, password } = credentials.info;
  await signedInPage(admin, await newSession(request, username, password, credentials.recoveryCode()), baseURL as string);
  await admin.getByLabel("Select every row on this page").check();
  const bar = admin.getByRole("region", { name: "Selected licences" });
  await bar.getByRole("button", { name: "Delete" }).click();
  await bar.getByRole("group", { name: "Delete" }).getByRole("button", { name: "Delete" }).click();
  await expect(admin.getByText("2 licences deleted.")).toBeVisible();
  await expect(admin.getByText("This page is out of date")).toHaveCount(0);
  expect((await api.get<{ data: unknown[] }>(`/licences?product=${product.id}`)).data).toHaveLength(0);
});
