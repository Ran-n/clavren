// Customers and products: created in a new first row (a product's code made from its name when
// left empty), edited in place, deleted or deactivated
// after an inline confirmation (a product with licences only after typing its code). Checked
// through the API.

import { createProduct, issue, unique } from "./data";
import { Device, expect, test } from "./fixtures";

test("customers are created, edited and deleted in the table", async ({ admin, api }) => {
  const name = unique("Dora");
  await admin.goto("/customers");
  await admin.getByRole("button", { name: "New customer" }).click();
  await admin.getByLabel("New customer's name").fill(name);
  await admin.getByLabel("New customer's email").press("Enter");
  await admin.getByLabel("Search by name or email").fill(name);
  const row = admin.getByRole("row").filter({ hasText: name });
  await expect(row).toHaveCount(1);

  await row.getByRole("button", { name: "Edit email" }).click();
  await row.getByLabel("email").fill("not an email");
  await row.getByLabel("email").press("Enter");
  await expect(row.getByRole("alert")).toHaveText("Email is not a valid address."); // in words; the field stays open
  await row.getByLabel("email").fill(`${name.toLowerCase()}@example.com`);
  await row.getByLabel("email").press("Enter");
  await expect(row.getByRole("button", { name: "Edit email" })).toHaveText(`${name.toLowerCase()}@example.com`);
  const { data } = await api.get<{ data: { id: string; email: string }[] }>(`/customers?q=${encodeURIComponent(name)}`);
  expect(data.map((customer) => customer.email)).toEqual([`${name.toLowerCase()}@example.com`]);

  await row.getByRole("button", { name: "Delete" }).click();
  await expect(admin.getByRole("group", { name: "Delete" })).toContainText("Their licences keep working; their name and email are erased.");
  await admin.getByRole("group", { name: "Delete" }).getByRole("button", { name: "Delete" }).click();
  await expect(row).toHaveCount(0);
  expect((await api.get<{ data: unknown[] }>(`/customers?q=${encodeURIComponent(name)}`)).data).toHaveLength(0);
});

test("products are created, their message edited, and deactivated after a warning", async ({ admin, api }) => {
  const existing = await createProduct(api);
  const name = unique("Gamma");
  await admin.goto("/products");
  await admin.getByRole("button", { name: "New product" }).click();
  await admin.getByLabel("New product's name").fill(name);
  await admin.getByLabel("New product's code").fill(name.toLowerCase());
  await admin.getByRole("button", { name: "Save" }).click();
  await expect(admin.getByRole("row").filter({ hasText: name })).toHaveCount(1);

  // Without a code, one is made from the name: lowercase, accents dropped.
  const automatic = unique("Gamma Ñ");
  await admin.getByRole("button", { name: "New product" }).click();
  await expect(admin.getByLabel("New product's code")).toHaveAttribute("placeholder", "Automatic");
  const save = admin.getByRole("button", { name: "Save" });
  await expect(save).toBeDisabled();
  await admin.getByLabel("New product's name").fill(automatic);
  await save.click();
  const made = automatic.toLowerCase().replace("ñ", "n").replace(/[^a-z0-9]+/g, "-");
  await expect(admin.getByRole("row").filter({ hasText: automatic }).getByRole("button", { name: "Edit code" })).toHaveText(made);

  const row = admin.getByRole("row").filter({ hasText: existing.name });
  await row.getByRole("button", { name: "Edit message template" }).click();
  const template = row.getByLabel("message template");
  await template.fill("Dear {customer_name},\nyour key: {key}");
  await template.press("Enter"); // a new line, not a save
  await template.press("Control+Enter");
  await expect(row.getByRole("button", { name: "Edit message template" })).toContainText("your key: {key}");

  await row.getByRole("button", { name: "Deactivate" }).click();
  await expect(admin.getByRole("group", { name: "Deactivate" })).toContainText("locks at its next check");
  await admin.getByRole("group", { name: "Deactivate" }).getByRole("button", { name: "Deactivate" }).click();
  await expect(row.getByText("Inactive")).toBeVisible();

  const saved = await api.get<{ is_active: boolean; message_template: string }>(`/products/${existing.id}`);
  expect(saved.is_active).toBe(false);
  expect(saved.message_template).toBe("Dear {customer_name},\nyour key: {key}");
});

test("a product without licences is deleted after a plain confirmation", async ({ admin, api }) => {
  const product = await createProduct(api);
  await admin.goto("/products");
  const row = admin.getByRole("row").filter({ hasText: product.name });
  await row.getByRole("button", { name: "Delete" }).click();
  const strip = admin.getByRole("group", { name: "Delete" });
  await expect(strip).toContainText(`Delete ${product.name}?`);
  await expect(strip.getByRole("textbox")).toHaveCount(0);
  await strip.getByRole("button", { name: "Delete" }).click();
  await expect(row).toHaveCount(0);
});

test("a product with licences is deleted with them once its code is typed", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id, { quantity: 3 })).licences;
  await new Device("PRODUCT-DELETE-PC").activate(request, product.code, issued[0]!.key, "203.0.113.43");
  await admin.goto("/products");
  const row = admin.getByRole("row").filter({ hasText: product.name });
  await row.getByRole("button", { name: "Delete" }).click();
  const strip = admin.getByRole("group", { name: "Delete" });
  await expect(strip).toContainText(`Delete ${product.name} and its 3 licences? 1 is in use: that computer locks at its next check.`);
  await strip.press("Escape");
  await expect(strip).toBeHidden();

  await row.getByRole("button", { name: "Delete" }).click();
  const confirm = strip.getByRole("button", { name: "Delete" });
  await expect(confirm).toBeDisabled();
  expect(Number(await confirm.evaluate((button) => getComputedStyle(button).opacity))).toBeLessThan(0.6);
  const word = strip.getByLabel(`Type ${product.code} to confirm`);
  await word.fill(product.code.toUpperCase());
  await expect(confirm, "exactly the code").toBeDisabled();
  await word.fill(product.code);
  await confirm.click();
  await expect(row).toHaveCount(0);

  for (const one of issued) {
    const gone = await api.get(`/licences/${one.id}`).then(
      () => false,
      () => true,
    );
    expect(gone).toBe(true);
  }
  await admin.goto(`/licences?product=${product.id}`);
  await expect(admin.getByText("No licence matches.")).toHaveCount(0); // the filter for a product that is gone is dropped
  await expect(admin.getByRole("row").filter({ hasText: product.name })).toHaveCount(0);
});
