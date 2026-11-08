// Everything the admin sets is remembered on the server and comes back after signing in again:
// theme, filters (exclusions too), order, page size, columns and the last page. A licence key is
// never saved.

import { createProduct } from "./data";
import { expect, test } from "./fixtures";

test("preferences are restored after signing in again", async ({ admin, credentials }) => {
  await admin.goto("/licences");
  await admin.getByLabel("State").selectOption("unused");
  await admin.getByLabel("Order").selectOption("expiry");
  await admin.getByLabel("Rows").selectOption("25");
  await admin.getByText("Columns").click();
  await admin.getByRole("checkbox", { name: "Notes" }).uncheck();
  const saved = admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await admin.getByLabel("Theme").selectOption("dark");
  expect((await saved).ok()).toBe(true);
  await admin.waitForTimeout(600); // the last change is saved shortly after it happens

  await admin.getByRole("button", { name: "Sign out" }).click();
  await expect(admin.getByRole("button", { name: "Sign in" })).toBeVisible();
  await admin.goto("/");
  await admin.getByLabel("Username").fill(credentials.info.username);
  await admin.getByLabel("Password").fill(credentials.info.password);
  await admin.getByLabel("Code from the authenticator app").fill(credentials.recoveryCode());
  await admin.getByRole("button", { name: "Sign in" }).click();

  await expect(admin).toHaveURL(/\/licences$/);
  await expect(admin.getByLabel("State")).toHaveValue("unused");
  await expect(admin.getByLabel("Order")).toHaveValue("expiry");
  await expect(admin.getByLabel("Rows")).toHaveValue("25");
  await expect(admin.getByRole("columnheader", { name: "Notes" })).toHaveCount(0);
  await expect(admin.getByRole("columnheader", { name: "Customer" })).toBeVisible();
  await expect(admin.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("exclusions come back after a reload, but a key typed in the hide box doesn't", async ({ admin }) => {
  const saved = () => admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await admin.goto("/licences");
  let put = saved();
  await admin.getByLabel("State").selectOption({ label: "Except revoked" });
  await put;
  put = saved();
  await admin.getByLabel("Hide key, customer or licence ID").fill("López");
  expect((await put).request().postData()).toContain("López");
  await admin.reload();
  await expect(admin.getByLabel("State")).toHaveValue("not:revoked");
  await expect(admin.getByLabel("Hide key, customer or licence ID")).toHaveValue("López");

  put = saved();
  await admin.getByLabel("Hide key, customer or licence ID").fill("brn 79yn 44xq w025 377z");
  expect((await put).request().postData()).not.toContain("79yn");
  await admin.reload();
  await expect(admin.getByLabel("Hide key, customer or licence ID")).toHaveValue("");
  await expect(admin.getByLabel("State")).toHaveValue("not:revoked");
});

test("a remembered product filter is dropped once that product is deleted", async ({ admin, api }) => {
  const product = await createProduct(api);
  const saved = () => admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await admin.goto("/licences");
  let put = saved();
  await admin.getByRole("combobox", { name: /^Product/ }).selectOption({ label: product.name });
  await put;
  await api.call("DELETE", `/products/${product.id}`);

  put = saved();
  await admin.reload();
  await expect(admin.getByRole("combobox", { name: /^Product/ })).toHaveValue("");
  expect((await put).request().postData()).not.toContain(product.id);
});
