// The Licences page and its customers: the customer link that hides a customer, the customer field
// (every customer A to Z, narrowing as you type, picked by mouse or keyboard, and a failed list with
// Retry), a row's customer set in place, and Enter in the field never issuing. Results are checked
// through the API.

import { createProduct, issue, licence, unique } from "./data";
import { expect, productFilter, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

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

// Regression: Enter in the customer field, with no suggestion highlighted, submitted the form:
// typing a new customer's name and pressing Enter issued licences at once.
test("Enter in the customer field never issues", async ({ admin, api }) => {
  const product = await createProduct(api);
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByRole("combobox", { name: "Customer (optional)" }).fill(unique("Typed then Enter"));
  const posts: string[] = [];
  admin.on("request", (request) => {
    if (request.method() === "POST") posts.push(request.url());
  });
  await panel.getByRole("combobox", { name: "Customer (optional)" }).press("Enter");
  await admin.evaluate(() => undefined); // a submit would have sent its request by now
  expect(posts, "Enter sends nothing").toEqual([]);
  await expect(panel.getByRole("button", { name: "Issue", exact: true })).toBeVisible();
  expect((await api.get<{ data: unknown[] }>(`/licences?product=${product.id}`)).data).toHaveLength(0);
});
