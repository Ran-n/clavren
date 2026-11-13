// Licence applications end to end: a product's applications opened on the Products page (with its
// key page link), its key page in each state with the instructions written on the Products page, an application sent on the public web form, approved on the Applications page (the
// customer filled in from it) and its key collected on the status page; a rejection's reason
// shown to the applicant; bulk deletion. Results are checked through the API.

import type { Page } from "@playwright/test";
import { createProduct, unique } from "./data";
import { expect, test, type AdminApi } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

type Listed = { data: { id: string; name: string; state: string; licence: string | null }[] };

let address = 0;
/** Each test's public requests come from their own address, so the five-an-hour limit never binds. */
async function ownAddress(page: Page): Promise<void> {
  address += 1;
  const from = `203.0.113.${100 + address}`;
  await page.route("**/api/v1/**", (route) => route.continue({ headers: { ...route.request().headers(), "cf-connecting-ip": from } }));
}

async function openProduct(api: AdminApi): Promise<{ id: string; code: string; name: string }> {
  const product = await createProduct(api);
  await api.call("PATCH", `/products/${product.id}`, { accepts_applications: true });
  return product;
}

/** Fills in and sends the web form for `code` as `name`; returns the status page's secret. */
async function apply(page: Page, code: string, name: string, email: string): Promise<string> {
  await page.goto(`/apply?product=${code}`);
  await page.getByLabel("Your name").fill(name);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Message (optional)").fill("For my class");
  await page.getByRole("button", { name: "Send application" }).click();
  await expect(page.getByText("Your application was sent.")).toBeVisible();
  await expect(page).toHaveURL(/\/apply\/status#.+/);
  return decodeURIComponent(new URL(page.url()).hash.slice(1));
}

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

test("an application sent on the web is approved, and its key collected on the status page", async ({ admin, api, browser }) => {
  const product = await openProduct(api);
  const name = unique("Ana");
  const email = `${name.toLowerCase()}@example.com`;
  const applicant = await browser.newPage();
  await ownAddress(applicant);
  const secret = await apply(applicant, product.code, name, email);
  await expect(applicant.getByText("waiting for the administrator")).toBeVisible();
  await applicant.getByRole("button", { name: "Copy link" }).click();

  await admin.goto("/applications");
  const row = admin.getByRole("row").filter({ hasText: name });
  await expect(row).toContainText("For my class");
  await expect(row).toContainText("Web form");
  await row.getByRole("button", { name: "Approve" }).click();
  const form = admin.getByRole("form", { name: "Approve application" });
  await expect(form.getByRole("combobox", { name: /^Customer/ })).toHaveValue(name);
  await expect(form.getByLabel("New customer's email (optional)")).toHaveValue(email);
  await form.getByLabel("Expires (optional)").fill("2030-06-30");
  await form.getByRole("button", { name: "Approve and issue" }).click();
  await expect(admin.getByText(`Approved: ${name}'s licence was issued.`)).toBeVisible();

  const { data } = await api.get<Listed>(`/applications?product=${product.id}&state=approved`);
  const licence = data.find((application) => application.name === name)!.licence!;
  const issued = await api.get<{ customer: { email: string }; expires_at: string }>(`/licences/${licence}?expand=customer`);
  expect(issued.customer.email).toBe(email);
  expect(issued.expires_at.slice(0, 10)).toBe("2030-06-30");
  const key = (await api.call<{ key: string }>("POST", `/licences/${licence}/reveal`, {})).key;

  await applicant.getByRole("button", { name: "Check again" }).click();
  await expect(applicant.getByText("Approved. Type this key")).toBeVisible();
  await expect(applicant.locator("code.key")).toHaveText(key);
  await expect(applicant.getByText("works until 30 Jun 2030")).toBeVisible();
  // the link alone brings it back, in a fresh page
  const later = await browser.newPage();
  await ownAddress(later);
  await later.goto(`/apply/status#${secret}`);
  await expect(later.locator("code.key")).toHaveText(key);
  await later.close();
  await applicant.close();
});

test("a rejection's reason reaches the applicant", async ({ admin, api, browser }) => {
  const product = await openProduct(api);
  const name = unique("Bo");
  const applicant = await browser.newPage();
  await ownAddress(applicant);
  await apply(applicant, product.code, name, `${name.toLowerCase()}@example.com`);

  await admin.goto("/applications");
  const row = admin.getByRole("row").filter({ hasText: name });
  await row.getByRole("button", { name: "Reject" }).click();
  const strip = admin.getByRole("group", { name: "Reject" });
  await strip.getByLabel("Reason (optional)").fill("Only for schools");
  await strip.getByRole("button", { name: "Reject" }).click();
  await expect(row).toHaveCount(0); // the list shows the waiting ones
  await admin.getByLabel("State").selectOption({ label: "Rejected" });
  await expect(admin.getByRole("row").filter({ hasText: name })).toContainText("Only for schools");
  await expect(admin.getByRole("row").filter({ hasText: name }).getByRole("button", { name: "Approve" })).toBeDisabled();

  await applicant.reload();
  await expect(applicant.getByText("wasn't approved")).toBeVisible();
  await expect(applicant.getByText("“Only for schools”")).toBeVisible();
  await applicant.close();
});

test("selected applications are deleted together", async ({ admin, api, request }) => {
  const product = await openProduct(api);
  const names = [unique("Cy"), unique("Di")];
  for (const [index, name] of names.entries()) {
    const response = await request.post("/api/v1/applications", {
      data: { product: product.code, name, email: `${name.toLowerCase()}@example.com` },
      headers: { "cf-connecting-ip": `198.51.100.${200 + index}` },
    });
    expect(response.status(), await response.text()).toBe(201);
  }
  await admin.goto("/applications");
  await admin.getByRole("combobox", { name: /^Product/ }).selectOption({ label: product.name });
  for (const name of names) await admin.getByRole("row").filter({ hasText: name }).getByLabel("Select this row").check();
  await admin.getByRole("group", { name: "Selected applications" }).getByRole("button", { name: "Delete" }).click();
  await admin.getByRole("group", { name: "Delete" }).getByRole("button", { name: "Delete" }).click();
  await expect(admin.getByText("2 applications deleted.")).toBeVisible();
  const { data } = await api.get<Listed>(`/applications?product=${product.id}&state=pending`);
  expect(data).toEqual([]);
});

test("the Overview says when applications are waiting", async ({ admin, api, request }) => {
  const product = await openProduct(api);
  const response = await request.post("/api/v1/applications", {
    data: { product: product.code, name: unique("Eva"), email: `${unique("eva")}@example.com` },
    headers: { "cf-connecting-ip": "198.51.100.250" },
  });
  expect(response.status()).toBe(201);
  await admin.goto("/");
  await admin.getByRole("link", { name: "Review applications" }).click();
  await expect(admin).toHaveURL(/\/applications$/);
  await expect(admin.getByRole("heading", { name: "Applications" })).toBeVisible();
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
