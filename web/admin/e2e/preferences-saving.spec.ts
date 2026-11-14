// When settings are saved, and what happens when they can't be: a theme that can't be saved still
// applies and says so; settings that couldn't be loaded are never saved over; a change made just
// before signing out is saved first; a setting that couldn't be saved keeps you signed in, with the
// error; a tab left open never undoes what another tab changed; and a page that changes nothing saves
// nothing.

import { DEFAULT_PREFERENCES, expect, lastPageSaved, test } from "./fixtures";
import { savedPreferences, savesNothing, themeButton } from "./preferences-helpers";

test("a theme that can't be saved still applies and says so", async ({ admin, api }) => {
  await admin.goto("/");
  await expect(themeButton(admin)).toBeVisible();
  await lastPageSaved(api, "/");
  await admin.route("**/api/admin/preferences", (route) =>
    route.request().method() === "PUT"
      ? route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: "internal_error", message: "" }) })
      : route.continue(),
  );
  try {
    await themeButton(admin).click();
    await expect(admin.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(admin.getByText("Your settings couldn't be saved")).toBeVisible();
  } finally {
    await admin.unroute("**/api/admin/preferences");
  }
});

// Regression: when the saved settings couldn't be loaded, the defaults were used and saved over
// them by the first change (or just by opening a page).
test("settings that couldn't be loaded are never saved over", async ({ admin, api }) => {
  await api.call("PUT", "/preferences", { version: 1, theme: "dark", tables: { licences: { order: "expiry" } } });
  let failed = false;
  await admin.route("**/api/admin/preferences", async (route) => {
    if (route.request().method() === "GET" && !failed) {
      failed = true;
      return route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: "internal_error", message: "" }) });
    }
    return route.continue();
  });
  const puts: string[] = [];
  admin.on("request", (request) => {
    if (request.url().endsWith("/api/admin/preferences") && request.method() === "PUT") puts.push(request.postData() ?? "");
  });
  try {
    await admin.clock.install();
    await admin.goto("/licences");
    await expect(admin.getByRole("alert").filter({ hasText: "couldn't be loaded" })).toContainText("Reload the page to try again.");
    await admin.getByLabel("Rows").selectOption("25");
    await savesNothing(admin);
    expect(puts).toHaveLength(0);
    const saved = await api.get<{ theme: string; tables: { licences?: { order?: string } } }>("/preferences");
    expect(saved.theme).toBe("dark");
    expect(saved.tables.licences?.order).toBe("expiry");
  } finally {
    await api.call("PUT", "/preferences", DEFAULT_PREFERENCES);
  }
});

// Regression: a change made just before signing out was dropped with the save timer.
test("a change made just before signing out is saved first", async ({ admin }) => {
  const sent: string[] = [];
  admin.on("request", (request) => {
    if (request.url().endsWith("/api/admin/preferences") && request.method() === "PUT") sent.push(`PUT ${request.postData() ?? ""}`);
    if (request.url().endsWith("/api/admin/session") && request.method() === "DELETE") sent.push("DELETE session");
  });
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await admin.getByLabel("Rows").selectOption("50");
  await admin.getByRole("button", { name: "Sign out" }).click(); // within the save delay
  await expect(admin.getByRole("button", { name: "Sign in" })).toBeVisible();
  const put = sent.findIndex((entry) => entry.startsWith("PUT") && entry.includes('"page_size":50'));
  expect(put, sent.join(" | ")).toBeGreaterThanOrEqual(0);
  expect(put).toBeLessThan(sent.indexOf("DELETE session"));
});

test("a setting that couldn't be saved keeps you signed in, with the error", async ({ admin, api }) => {
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await lastPageSaved(api, "/licences");
  await admin.route("**/api/admin/preferences", (route) =>
    route.request().method() === "PUT"
      ? route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: "internal_error", message: "" }) })
      : route.continue(),
  );
  await admin.getByLabel("Rows").selectOption("25");
  await admin.getByRole("button", { name: "Sign out" }).click();
  await expect(admin.getByText("Something went wrong on the server.").first()).toBeVisible();
  await expect(admin.getByRole("button", { name: "Sign out" })).toBeVisible();
  await admin.unroute("**/api/admin/preferences");
});

// Regression: every save sent the whole record from this page's memory, so a tab opened before
// another one changed the theme put the old theme back with its next change.
test("a tab left open never undoes what another tab changed", async ({ admin, api }) => {
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  const other = await admin.context().newPage();
  await other.goto("/");
  const saved = savedPreferences(other);
  await themeButton(other).click(); // System → Light
  await themeButton(other).click(); // Light → Dark
  expect((await saved).ok()).toBe(true);
  await expect.poll(async () => (await api.get<{ theme: string }>("/preferences")).theme).toBe("dark");

  const stale = savedPreferences(admin);
  await admin.getByLabel("Rows").selectOption("25");
  expect((await stale).ok()).toBe(true);
  const after = await api.get<{ theme: string; tables: { licences?: { page_size?: number } } }>("/preferences");
  expect(after.theme).toBe("dark");
  expect(after.tables.licences?.page_size).toBe(25);
  await other.close();
});

// Regression: opening Licences saved the settings again each time, though nothing had changed.
test("opening a page that changes nothing saves nothing", async ({ admin, api }) => {
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await lastPageSaved(api, "/licences");
  const puts: string[] = [];
  admin.on("request", (request) => {
    if (request.method() === "PUT" && request.url().endsWith("/api/admin/preferences")) puts.push(request.url());
  });
  await admin.clock.install();
  await admin.reload();
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await savesNothing(admin);
  expect(puts).toHaveLength(0);
});
