// Everything the admin sets is remembered on the server and comes back after signing in again:
// theme, filters, order, page size, columns and the last page.

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
