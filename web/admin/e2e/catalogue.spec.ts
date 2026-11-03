// Customers and products: created in a new first row, edited in place, deleted or deactivated
// after an inline confirmation. Checked through the API.

import { createProduct, unique } from "./data";
import { expect, test } from "./fixtures";

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
  await expect(row.getByRole("alert")).toBeVisible(); // the reason, in words, and the field stays open
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
