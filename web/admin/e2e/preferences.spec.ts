// Settings the admin sets are remembered on the server and come back after signing in again: the
// theme (its button, by mouse and keyboard), a table's filters (exclusions too), order, page size and
// columns, the Issue panel, and a remembered product filter, which is dropped once its product is
// deleted. A licence key is never saved. Saving, and what happens when it fails, is in
// preferences-saving.spec.ts.

import { createProduct } from "./data";
import { expect, lastPageSaved, settingsSaved, test } from "./fixtures";
import { savedPreferences, themeButton } from "./preferences-helpers";

test("preferences are restored after signing in again", async ({ admin, api, credentials }) => {
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
  await settingsSaved(api, (settings) => settings.theme === "dark"); // the last change, saved a moment after it

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
  await lastPageSaved(api, "/"); // so the next save is the theme's
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
