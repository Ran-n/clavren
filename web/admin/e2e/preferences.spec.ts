// Everything the admin sets is remembered on the server and comes back after signing in again:
// theme, filters (exclusions too), order, page size, columns and the last page. A licence key is
// never saved.

import type { Page } from "@playwright/test";
import { createProduct } from "./data";
import { DEFAULT_PREFERENCES, expect, test } from "./fixtures";

const themeButton = (page: Page) => page.getByRole("button", { name: /^Theme:/ });
const savedPreferences = (page: Page) =>
  page.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");

test("preferences are restored after signing in again", async ({ admin, credentials }) => {
  await admin.goto("/licences");
  await admin.getByLabel("State").selectOption("unused");
  await admin.getByLabel("Order").selectOption("expiry");
  await admin.getByLabel("Rows").selectOption("25");
  await admin.getByText("Columns").click();
  await admin.getByRole("checkbox", { name: "Notes" }).uncheck();
  const saved = admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await themeButton(admin).click(); // System → Light
  await themeButton(admin).click(); // Light → Dark
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
  await expect(themeButton(admin)).toHaveAccessibleName("Theme: Dark");
});

test("the theme button cycles system, light, dark and back", async ({ admin, api }) => {
  await admin.goto("/");
  await expect(themeButton(admin)).toHaveAccessibleName("Theme: System");
  await expect(themeButton(admin)).toHaveAttribute("title", "Theme: System (click for Light)");
  await admin.waitForTimeout(600); // the last page is saved, so the next save is the theme's
  for (const [name, attribute] of [["Light", "light"], ["Dark", "dark"], ["System", null]] as const) {
    const saved = savedPreferences(admin);
    await themeButton(admin).click();
    await expect(themeButton(admin)).toHaveAccessibleName(`Theme: ${name}`);
    if (attribute) await expect(admin.locator("html")).toHaveAttribute("data-theme", attribute);
    else await expect(admin.locator("html")).not.toHaveAttribute("data-theme", /./);
    expect((await saved).ok()).toBe(true);
    expect((await api.get<{ theme: string }>("/preferences")).theme).toBe(name.toLowerCase());
  }
});

test("the theme button works from the keyboard", async ({ admin }) => {
  await admin.goto("/");
  await themeButton(admin).focus();
  await themeButton(admin).press("Enter");
  await expect(themeButton(admin)).toHaveAccessibleName("Theme: Light");
  await themeButton(admin).press(" ");
  await expect(themeButton(admin)).toHaveAccessibleName("Theme: Dark");
  await expect(themeButton(admin)).toBeFocused();
});

test("a theme that can't be saved still applies and says so", async ({ admin }) => {
  await admin.goto("/");
  await expect(themeButton(admin)).toBeVisible();
  await admin.waitForTimeout(600); // the last page is saved
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
    await admin.goto("/licences");
    await expect(admin.getByRole("alert").filter({ hasText: "couldn't be loaded" })).toContainText("Reload the page to try again.");
    await admin.getByLabel("Rows").selectOption("25");
    await admin.waitForTimeout(800); // longer than the save delay
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

test("a setting that couldn't be saved keeps you signed in, with the error", async ({ admin }) => {
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await admin.waitForTimeout(600); // the last page is saved
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

test("a remembered product filter on Activations is dropped once that product is deleted", async ({ admin, api }) => {
  const product = await createProduct(api);
  const saved = () => admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await admin.goto("/activations");
  let put = saved();
  await admin.getByRole("combobox", { name: /^Product/ }).selectOption({ label: product.name });
  await put;
  await api.call("DELETE", `/products/${product.id}`);

  put = saved();
  await admin.reload();
  await expect(admin.getByRole("combobox", { name: /^Product/ })).toHaveValue("");
  expect((await put).request().postData()).not.toContain(product.id);
});

test("the Issue panel is remembered open, with its product, quantity and expiry but never its customer or notes", async ({ admin, api }) => {
  const product = await createProduct(api);
  const bodies: string[] = [];
  admin.on("request", (request) => {
    if (request.url().endsWith("/api/admin/preferences") && request.method() === "PUT") bodies.push(request.postData() ?? "");
  });
  const saved = () => admin.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByLabel("How many").fill("3");
  await panel.getByLabel("Expires (optional)").fill("2027-12-31");
  await panel.getByRole("combobox", { name: "Customer (optional)" }).fill("Remembered Nobody");
  let put = saved();
  await panel.getByLabel("Notes (optional)").fill("Never stored note");
  await panel.getByLabel("How many").fill("4");
  await put;

  await admin.reload();
  await expect(panel).toBeVisible();
  await expect(panel.getByLabel("Product")).toHaveValue(product.id);
  await expect(panel.getByLabel("How many")).toHaveValue("4");
  await expect(panel.getByLabel("Expires (optional)")).toHaveValue("2027-12-31");
  await expect(panel.getByRole("combobox", { name: "Customer (optional)" })).toHaveValue("");
  expect(bodies.join("\n")).not.toContain("Remembered Nobody");
  expect(bodies.join("\n")).not.toContain("Never stored note");

  put = saved();
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("heading", { name: "4 licences issued" })).toBeVisible();
  await put;
  await admin.reload();
  await expect(admin.getByRole("button", { name: "Issue licences" })).toBeVisible();
  await expect(panel).toHaveCount(0);
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
test("opening a page that changes nothing saves nothing", async ({ admin }) => {
  await admin.goto("/licences");
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await admin.waitForTimeout(600); // the last page is saved
  const puts: string[] = [];
  admin.on("request", (request) => {
    if (request.method() === "PUT" && request.url().endsWith("/api/admin/preferences")) puts.push(request.url());
  });
  await admin.reload();
  await expect(admin.getByLabel("Rows")).toBeVisible();
  await admin.waitForTimeout(800); // longer than the save delay
  expect(puts).toHaveLength(0);
});
