// The Activations page (revoking a computer resets its licence), the audit log's filters, and
// the Overview.

import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, test } from "./fixtures";

test("revoking a computer on the Activations page frees its licence", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const customer = unique("Eva");
  const issued = (await issue(api, product.id, { customer: { name: customer } })).licences[0]!;
  const device = unique("EVA-PC");
  await new Device(device).activate(request, product.code, issued.key, "203.0.113.31");

  await admin.goto("/activations");
  await admin.getByLabel("Search by computer, platform or version").fill(device);
  const row = admin.getByRole("row").filter({ hasText: device });
  await expect(row).toContainText(product.name);
  await expect(row).toContainText(customer);
  await row.getByRole("button", { name: "Revoke" }).click();
  await admin.getByRole("group", { name: "Revoke" }).getByRole("button", { name: "Revoke" }).click();
  await expect(row).toHaveCount(0); // the list shows computers in use
  expect((await licence(api, issued.id)).state).toBe("unused");
});

test("the audit log names who did what, and filters by action and admin", async ({ admin, api, credentials }) => {
  const product = await createProduct(api);
  await issue(api, product.id);
  await admin.goto("/audit");
  await admin.getByLabel("Action").selectOption({ label: "Licences" });
  const first = admin.getByRole("row").nth(1);
  await expect(first).toContainText("Licence issued");
  await first.getByRole("button", { name: credentials.info.username }).click();
  await expect(admin.getByText("Showing one admin.")).toBeVisible();
  await admin.getByRole("button", { name: "Show everything" }).click();
  await expect(admin.getByText("Showing one admin.")).toBeHidden();
});

test("the Overview counts licences and warns that there is no backup", async ({ admin }) => {
  await admin.goto("/");
  await expect(admin.getByRole("link", { name: /Unused/ })).toBeVisible();
  await expect(admin.getByText("There is no backup yet.")).toBeVisible();
  await admin.getByRole("link", { name: /Unused/ }).click();
  await expect(admin).toHaveURL(/\/licences$/);
  await expect(admin.getByLabel("State")).toHaveValue("unused");
});
