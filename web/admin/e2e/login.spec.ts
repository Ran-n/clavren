// Signing in with the authenticator app, refusals, signing out, a session that ends while the
// dashboard is open, and one replaced by signing in again in another tab.

import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, newSession, signedInPage, test } from "./fixtures";
import { SERVERS } from "./servers";

const CHECK_REQUEST = join(SERVERS.flows.dataDir, "updates", "check-request");
const OUT_OF_DATE = "This page is out of date";

/** The Overview, open and idle: its last page is saved, so Check now is the next change. */
async function overviewOpen(admin: Page): Promise<void> {
  await admin.goto("/");
  await expect(admin.getByRole("button", { name: "Check now" })).toBeEnabled();
  await admin.waitForTimeout(600);
}

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

// Regression: signing out of a session that had already ended answered "Your session has ended"
// and left the dashboard on screen, however often Sign out was pressed.
test("signing out of a session that already ended goes back to signing in", async ({ admin, api }) => {
  await admin.goto("/");
  await expect(admin.getByRole("heading", { name: "Overview" })).toBeVisible();
  await admin.waitForTimeout(600); // the last page is saved by now: nothing else asks the server
  await api.call("DELETE", "/session");
  await admin.getByRole("button", { name: "Sign out" }).click();
  await expect(admin.getByRole("button", { name: "Sign in" })).toBeVisible();
  await expect(admin.getByText("This page is out of date")).toHaveCount(0);
});

test.describe("a session replaced in another tab", () => {
  test.beforeEach(() => rmSync(CHECK_REQUEST, { force: true }));
  test.afterEach(() => rmSync(CHECK_REQUEST, { force: true }));

  // Regression: signing in again in another tab left this page with the old session's CSRF token,
  // so Check now (and every other change) answered "This page is out of date".
  test("a change after signing in again in another tab goes through", async ({ admin, credentials, request, baseURL }) => {
    await overviewOpen(admin);
    const { username, password } = credentials.info;
    await signedInPage(admin, await newSession(request, username, password, credentials.recoveryCode()), baseURL as string);
    await admin.getByRole("button", { name: "Check now" }).click();
    await expect.poll(() => existsSync(CHECK_REQUEST)).toBe(true);
    await expect(admin.getByRole("button", { name: "Check now" })).toHaveAttribute("aria-busy", "true");
    await expect(admin.getByText(OUT_OF_DATE)).toHaveCount(0);
  });

  test("a change after another admin signed in in another tab switches to their account", async ({ admin, credentials, request, baseURL }) => {
    const other = credentials.info.other;
    if (!other) throw new Error("the flows server has no second admin");
    await overviewOpen(admin);
    await expect(admin.locator(".app-header__user")).toHaveText(credentials.info.username);
    await signedInPage(admin, await newSession(request, other.username, other.password, credentials.otherRecoveryCode()), baseURL as string);
    await admin.getByRole("button", { name: "Check now" }).click();
    await expect(admin.getByText("You signed in as another admin in another tab. This page now shows that account; try again.")).toBeVisible();
    await expect(admin.locator(".app-header__user")).toHaveText(other.username);
    expect(existsSync(CHECK_REQUEST), "the refused check was not sent as the other admin").toBe(false);
    // the notice is about this page; elsewhere it is old news
    await admin.getByRole("link", { name: "Licences" }).click();
    await expect(admin.getByText("You signed in as another admin")).toHaveCount(0);
  });

  test("a change whose session ended meanwhile goes back to signing in", async ({ admin, credentials, request, baseURL }) => {
    const other = credentials.info.other;
    if (!other) throw new Error("the flows server has no second admin");
    await overviewOpen(admin);
    await signedInPage(admin, await newSession(request, other.username, other.password, credentials.otherRecoveryCode()), baseURL as string);
    // the session ends between the refusal and asking for the new token
    await admin.route("**/api/admin/session", (route) =>
      route.request().method() === "GET"
        ? route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ error: "unauthorized", message: "" }) })
        : route.continue(),
    );
    await admin.getByRole("button", { name: "Check now" }).click();
    await expect(admin.getByText("Your session has ended. Sign in again.")).toBeVisible();
    await expect(admin.getByRole("button", { name: "Sign in" })).toBeVisible();
    expect(existsSync(CHECK_REQUEST)).toBe(false);
  });

  test("a change refused again with the new token is sent only twice and says the page is out of date", async ({ admin }) => {
    await overviewOpen(admin);
    let checks = 0;
    let sessionReads = 0;
    await admin.route("**/api/admin/updates/check", (route) => {
      checks += 1;
      return route.fulfill({ status: 403, contentType: "application/json", body: JSON.stringify({ error: "csrf_failed", message: "" }) });
    });
    admin.on("request", (sent) => {
      if (sent.method() === "GET" && new URL(sent.url()).pathname === "/api/admin/session") sessionReads += 1;
    });
    await admin.getByRole("button", { name: "Check now" }).click();
    await expect(admin.getByText("This page is out of date. Reload it and try again.")).toBeVisible();
    expect(checks, "the first try and one retry, no loop").toBe(2);
    expect(sessionReads, "one request for the current token").toBe(1);
  });
});
