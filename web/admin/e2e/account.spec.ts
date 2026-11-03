// The Account page: password, recovery codes, a new authenticator app, other sessions. Each
// change is checked by signing in with what it produced.

import { request } from "@playwright/test";
import { expect, test } from "./fixtures";

async function signInWorks(baseURL: string, username: string, password: string, code: string): Promise<boolean> {
  const context = await request.newContext({ baseURL });
  const response = await context.post("/api/admin/session", { data: { username, password, code } });
  await context.dispose();
  return response.ok();
}

test("new recovery codes replace the old ones", async ({ admin, credentials, baseURL }) => {
  await admin.goto("/account");
  const card = admin.getByRole("form", { name: "Recovery codes" });
  await card.getByLabel("Password").fill(credentials.info.password);
  await card.getByLabel("Code from the authenticator app").fill(credentials.totp());
  await card.getByRole("button", { name: "Make new recovery codes" }).click();
  const shown = card.getByRole("list", { name: "New recovery codes" }).locator("code");
  await expect(shown).toHaveCount(10);
  const codes = await shown.allTextContents();
  const old = credentials.recoveryCode();
  credentials.replaceRecoveryCodes(codes);
  expect(await signInWorks(baseURL as string, credentials.info.username, credentials.info.password, old), "an old code").toBe(false);
});

test("moving to a new authenticator app", async ({ admin, credentials, baseURL }) => {
  await admin.goto("/account");
  await admin.getByRole("button", { name: "Move to a new authenticator app" }).click();
  const card = admin.getByRole("form", { name: "Authenticator app" });
  await expect(card.getByRole("img", { name: "QR code for the authenticator app" })).toBeVisible();
  const secret = (await card.locator("code.secret").textContent()) as string;
  await card.getByLabel("Password").fill(credentials.info.password);
  await card.getByLabel("Code from the new app").fill(credentials.confirmNewSecret(secret));
  await card.getByRole("button", { name: "Confirm the new app" }).click();
  await expect(admin.getByText("Done: from now on only the new app's codes work.")).toBeVisible();
  expect(await signInWorks(baseURL as string, credentials.info.username, credentials.info.password, credentials.totp())).toBe(true);
});

test("changing the password, and signing out everywhere else", async ({ admin, credentials }) => {
  const original = credentials.info.password;
  const changed = `${original} changed`;
  await admin.goto("/account");
  const card = admin.getByRole("form", { name: "Password" });
  await card.getByLabel("Current password").fill(original);
  await card.getByLabel("New password", { exact: true }).fill("too short");
  await expect(card.getByRole("button", { name: "Change password" })).toBeDisabled();
  await card.getByLabel("New password", { exact: true }).fill(changed);
  await card.getByLabel("New password again").fill(changed);
  await card.getByRole("button", { name: "Change password" }).click();
  await expect(card.getByText("Password changed.")).toBeVisible();

  // And back, so the other specs keep signing in.
  await card.getByLabel("Current password").fill(changed);
  await card.getByLabel("New password", { exact: true }).fill(original);
  await card.getByLabel("New password again").fill(original);
  await card.getByRole("button", { name: "Change password" }).click();
  await expect(card.getByText("Password changed.")).toBeVisible();

  const sessions = admin.getByRole("form", { name: "Other sessions" });
  await sessions.getByRole("button", { name: "Sign out everywhere else" }).click();
  await expect(sessions.getByRole("status")).toBeVisible();
  await expect(admin.getByRole("heading", { name: "Account" })).toBeVisible(); // still signed in here
});
