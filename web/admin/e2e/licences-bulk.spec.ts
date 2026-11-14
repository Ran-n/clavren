// The Licences page in bulk: the bulk bar changing every selected licence, its expiry date and its
// price, bulk delete with the server's counts, a bulk delete after signing in again in another tab,
// the selection kept when one row is unticked, and the filters that hide what they name (in the list,
// in bulk and in the CSV). Results are checked through the API.

import { readFileSync } from "node:fs";
import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, newSession, productFilter, settingsSaved, signedInPage, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

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

test("the bulk bar sets one expiry date on the selected licences, or none", async ({ admin, api }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id, { quantity: 3, expires_at: "2028-01-31T23:59:59Z" })).licences;
  await admin.goto("/licences");
  await productFilter(admin).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: "31 Jan 2028" });
  await expect(rows).toHaveCount(3);
  const bar = admin.getByRole("region", { name: "Selected licences" });
  const expiries = async () => (await Promise.all(issued.map((one) => licence(api, one.id)))).map((one) => one.expires_at).sort();

  await rows.nth(0).getByLabel("Select this row").check();
  await rows.nth(1).getByLabel("Select this row").check();
  await bar.getByRole("button", { name: "Edit expiry" }).click();
  await bar.getByLabel("New expiry (empty: permanent)").fill("2029-02-28");
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(admin.getByText("2 licences updated.")).toBeVisible();
  await expect(admin.getByRole("row").filter({ hasText: "28 Feb 2029" })).toHaveCount(2);
  expect(await expiries()).toEqual(["2028-01-31T23:59:59Z", "2029-02-28T23:59:59Z", "2029-02-28T23:59:59Z"]);

  // an empty date makes them permanent
  await rows.getByLabel("Select this row").check();
  await bar.getByRole("button", { name: "Edit expiry" }).click();
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(admin.getByText("1 licence updated.")).toBeVisible();
  await expect(rows).toHaveCount(0);
  expect(await expiries()).toEqual(["2029-02-28T23:59:59Z", "2029-02-28T23:59:59Z", null]);
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
  // once the filter is saved, Delete is the next change
  await settingsSaved(api, (settings) => settings.tables.licences?.filters?.product === product.id);
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

test("the bulk bar sets a price on the selected licences, keeping test licences without one", async ({ admin, api }) => {
  const product = await createProduct(api);
  await issue(api, product.id, { quantity: 2 });
  await api.call("POST", "/licences", { product: product.id, test: true });
  await admin.goto(`/licences?product=${product.id}`);
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(3);
  for (const index of [0, 1, 2]) await rows.nth(index).getByLabel("Select this row").check();
  const bar = admin.getByRole("region", { name: "Selected licences" });
  await bar.getByRole("button", { name: "Edit price" }).click();
  await bar.getByLabel("New price", { exact: true }).fill("9.50");
  await bar.getByRole("button", { name: "Apply" }).click();
  await expect(admin.getByRole("status")).toHaveText("2 licences updated. 1 test licence kept no price.");
  const { data } = await api.get<{ data: { is_test: boolean; price: unknown }[] }>(`/licences?product=${product.id}`);
  for (const licence of data) {
    expect(licence.price).toEqual(licence.is_test ? null : { amount: 950, currency: "EUR" });
  }
});
