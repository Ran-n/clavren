// Signing in with the authenticator app, refusals, signing out, and a session that ends while the
// dashboard is open.

import { expect, test } from "./fixtures";

test("signing in takes the right code, and signing out ends the session", async ({ page, credentials }) => {
  await page.goto("/");
  await page.getByLabel("Username").fill(credentials.info.username);
  await page.getByLabel("Password").fill(credentials.info.password);
  await page.getByLabel("Code from the authenticator app").fill("000000");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("alert")).toHaveText("The username, password or code is wrong.");
  await expect(page.getByLabel("Code from the authenticator app")).toHaveValue("");

  await page.getByLabel("Code from the authenticator app").fill(credentials.totp());
  await page.getByRole("button", { name: "Sign in" }).click();
  // Signed in, on the last page this admin had open.
  await expect(page.getByRole("button", { name: "Sign out" })).toBeVisible();
  await expect(page.getByText(credentials.info.username, { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Sign out" }).click();
  await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
});

test("a session that ends while the dashboard is open goes back to signing in", async ({ admin, api }) => {
  await admin.goto("/");
  await expect(admin.getByRole("heading", { name: "Overview" })).toBeVisible();
  await api.call("DELETE", "/session"); // the same session, ended from elsewhere
  await admin.getByRole("link", { name: "Licences" }).click();
  await expect(admin.getByText("Your session has ended. Sign in again.")).toBeVisible();
  await expect(admin.getByRole("button", { name: "Sign in" })).toBeVisible();
});
