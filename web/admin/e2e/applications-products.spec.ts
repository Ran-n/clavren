// Licence applications from the product's side: a product's applications opened on the Products page,
// with a link to its key page; its key page in each state, with the instructions written on the
// Products page; and the key page when the server can't be reached or a status link is mangled.
// Results are checked through the API.

import { createProduct } from "./data";
import { expect, test } from "./fixtures";
import { ownAddress } from "./applications-helpers";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

test("a product's applications open on the Products page, with a link to its key page", async ({ admin, api }) => {
  const product = await createProduct(api);
  await admin.goto("/products");
  const row = admin.getByRole("row").filter({ hasText: product.name });
  await expect(row.getByText("Closed")).toBeVisible();
  await row.getByRole("button", { name: "Copy key page link" }).click();
  await expect(row.getByRole("button", { name: "Copied" })).toBeVisible();
  expect(await admin.evaluate(() => navigator.clipboard.readText())).toMatch(new RegExp(`/apply\\?product=${product.code}$`));
  await admin.evaluate(() => navigator.clipboard.writeText(""));
  await row.getByRole("button", { name: "Open", exact: true }).click();
  await row.getByRole("button", { name: "Copy key page link" }).click();
  await expect(row.getByRole("button", { name: "Copied" })).toBeVisible();
  expect(await admin.evaluate(() => navigator.clipboard.readText())).toMatch(new RegExp(`/apply\\?product=${product.code}$`));
  expect((await api.get<{ accepts_applications: boolean }>(`/products/${product.id}`)).accepts_applications).toBe(true);
  await row.getByRole("button", { name: "Close", exact: true }).click();
  await expect(row.getByText("Closed")).toBeVisible();
  expect((await api.get<{ accepts_applications: boolean }>(`/products/${product.id}`)).accepts_applications).toBe(false);
});

const INSTRUCTIONS = "Write to licences@example.com.\nOr buy one at https://example.com/shop.";

test("how to get a key, written on the Products page, shows on the key page with its links", async ({ admin, api, browser }) => {
  const product = await createProduct(api);
  await admin.goto("/products");
  const row = admin.getByRole("row").filter({ hasText: product.name });
  await row.getByRole("button", { name: "Edit how to get a key" }).click();
  await row.getByLabel("how to get a key").fill(INSTRUCTIONS);
  await row.getByLabel("how to get a key").press("Control+Enter");
  await expect(row.getByText("Or buy one at https://example.com/shop.")).toBeVisible();
  expect((await api.get<{ key_instructions: string }>(`/products/${product.id}`)).key_instructions).toBe(INSTRUCTIONS);

  const visitor = await browser.newPage();
  await ownAddress(visitor);
  await visitor.goto(`/apply?product=${product.code}`);
  await expect(visitor.getByRole("heading", { name: `Get a licence key for ${product.name}` })).toBeVisible();
  await expect(visitor).toHaveTitle(`Get a licence key for ${product.name}`);
  await expect(visitor.getByText("Write to licences@example.com.")).toBeVisible();
  const link = visitor.getByRole("link", { name: "https://example.com/shop" });
  await expect(link).toHaveAttribute("href", "https://example.com/shop");
  await expect(link).toHaveAttribute("target", "_blank");
  await expect(link).toHaveAttribute("rel", "noopener noreferrer");
  await expect(visitor.getByRole("button", { name: "Send application" })).toHaveCount(0);

  // open: the instructions, then the form for this product only
  await api.call("PATCH", `/products/${product.id}`, { accepts_applications: true });
  await visitor.reload();
  await expect(visitor.getByRole("heading", { name: "Apply for a licence" })).toBeVisible();
  await expect(visitor.getByText("Write to licences@example.com.")).toBeVisible();
  await expect(visitor.getByLabel("Product")).toHaveCount(0);
  await expect(visitor.getByRole("button", { name: "Send application" })).toBeVisible();

  // closed with no instructions
  await api.call("PATCH", `/products/${product.id}`, { accepts_applications: false, key_instructions: null });
  await visitor.reload();
  await expect(visitor.getByText(`Ask the seller of ${product.name} for a licence key.`)).toBeVisible();

  // inactive, and unknown: the same notice, no form
  await api.call("PATCH", `/products/${product.id}`, { is_active: false });
  for (const code of [product.code, "no-such-product"]) {
    await visitor.goto(`/apply?product=${code}`);
    await expect(visitor.getByText("This product isn't available.")).toBeVisible();
    await expect(visitor.getByRole("button", { name: "Send application" })).toHaveCount(0);
  }
  await visitor.close();
});

test("the key page says so when the server can't be reached", async ({ page }) => {
  await page.route("**/api/v1/applications/products/*", (route) => route.abort());
  await page.goto("/apply?product=avalio-lite");
  await expect(page.getByRole("alert")).toHaveText("The licence server can't be reached. Check your connection and try again.");
});

// Regression: a stray % in the link's code threw while the page was drawn, leaving "Reload the
// page", which could never help.
test("a mangled status link reads as incomplete", async ({ page }) => {
  await page.goto("/apply/status#abc%zz");
  await expect(page.getByRole("alert")).toContainText("This link is incomplete");
});
