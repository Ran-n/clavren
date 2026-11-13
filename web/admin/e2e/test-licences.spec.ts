// Test licences: a new product's comes with its key on screen (Copy key), more are issued with the
// Issue panel's tick, the Kind filter keeps or hides them, the Overview's numbers link past them,
// and revoking one of a test licence's computers leaves the others. Results are checked through
// the API.

import { createProduct, unique } from "./data";
import { Device, expect, test } from "./fixtures";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

type Listed = { data: { id: string; is_test: boolean; active_computers: number }[] };

test("a new product shows its test licence's key, which copies and is stored as a test licence", async ({ admin, api }) => {
  const name = unique("Lab");
  await admin.goto("/products");
  await admin.getByRole("button", { name: "New product" }).click();
  await admin.getByLabel("New product's name").fill(name);
  await admin.getByLabel("New product's name").press("Enter");

  const notice = admin.getByRole("status").filter({ hasText: `${name} was created with a test licence` });
  await expect(notice).toBeVisible();
  const key = (await notice.locator("code.key").textContent()) ?? "";
  expect(key).toMatch(/^BRN(-[0-9A-Z]{4}){4}$/);
  await notice.getByRole("button", { name: "Copy key" }).click();
  await expect(notice.getByRole("button", { name: "Copied" })).toBeVisible();
  expect(await admin.evaluate(() => navigator.clipboard.readText())).toBe(key);

  const product = (await api.get<{ data: { id: string; name: string }[] }>("/products?limit=100")).data.find((p) => p.name === name)!;
  const { data } = await api.get<Listed>(`/licences?product=${product.id}`);
  expect(data.map((licence) => licence.is_test)).toEqual([true]);
  expect((await api.call<{ key: string }>("POST", `/licences/${data[0]!.id}/reveal`, {})).key).toBe(key);

  await notice.getByRole("link", { name: "Open it" }).click();
  await expect(admin.getByRole("heading", { name: /Licence/ })).toContainText("Test");
  await admin.goto("/products");
  await expect(admin.getByText(`${name} was created with a test licence`)).toHaveCount(0); // gone once the page is left
});

test("test licences are issued with the panel's tick and kept or hidden by the Kind filter", async ({ admin, api }) => {
  const product = await createProduct(api);
  await api.call("POST", "/licences", { product: product.id });
  await admin.goto("/licences");
  await admin.getByRole("button", { name: "Issue licences" }).click();
  const panel = admin.getByRole("region", { name: "Issue licences" });
  await panel.getByLabel("Product").selectOption({ label: product.name });
  await panel.getByLabel("How many").fill("2");
  await panel.getByLabel(/^Test licences/).check();
  await panel.getByRole("button", { name: "Issue", exact: true }).click();
  await expect(panel.getByRole("heading", { name: "2 test licences issued" })).toBeVisible();
  const keys = await panel.getByRole("list", { name: "New keys" }).locator("code").allTextContents();
  await expect(panel.getByText("The 2 keys are on the clipboard.")).toBeVisible();
  // Chrome on Windows stores copied line breaks as CRLF, as the Windows clipboard does
  expect((await admin.evaluate(() => navigator.clipboard.readText())).replaceAll("\r\n", "\n")).toBe(keys.join("\n"));
  await panel.getByRole("button", { name: "Close" }).click();

  const { data } = await api.get<Listed>(`/licences?product=${product.id}`);
  expect(data.map((licence) => licence.is_test).sort()).toEqual([false, true, true]);

  await admin.getByRole("combobox", { name: /^Product/ }).selectOption({ label: product.name });
  const rows = admin.getByRole("row").filter({ hasText: product.name });
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "Test" })).toHaveCount(2);
  await admin.getByLabel("Kind").selectOption({ label: "Test licences" });
  await expect(rows).toHaveCount(2);
  await admin.getByLabel("Kind").selectOption({ label: "Except test licences" });
  await expect(rows).toHaveCount(1);
  await expect(rows.filter({ hasText: "Test" })).toHaveCount(0);

  // the next panel starts unticked, so test licences are never issued by mistake
  await admin.getByRole("button", { name: "Issue licences" }).click();
  await expect(panel.getByLabel(/^Test licences/)).not.toBeChecked();
});

test("the Overview's licence numbers open the list without test licences", async ({ admin }) => {
  await admin.goto("/");
  await expect(admin.getByText("Test licences, their computers and their checks aren't counted here.")).toBeVisible();
  await admin.getByRole("link", { name: /Unused/ }).click();
  await expect(admin.getByLabel("State")).toHaveValue("unused");
  await expect(admin.getByLabel("Kind")).not.toHaveValue("");
  await expect(admin.getByLabel("Kind").locator("option:checked")).toHaveText("Except test licences");
});

test("revoking one computer of a test licence leaves its other computers", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const issued = await api.call<{ licences: { id: string; key: string }[] }>("POST", "/licences", { product: product.id, test: true });
  const licence = issued.licences[0]!;
  const first = new Device(unique("LAB-PC"));
  const second = new Device(unique("LAB-VM"));
  await first.activate(request, product.code, licence.key, "203.0.113.71");
  await second.activate(request, product.code, licence.key, "203.0.113.72");

  await admin.goto(`/licences/${licence.id}`);
  await expect(admin.getByText("In use on 2 computers")).toBeVisible();
  await admin.goto("/activations");
  await admin.getByLabel("Kind").selectOption({ label: "Test licences" });
  const row = admin.getByRole("row").filter({ hasText: first.name });
  await expect(row.getByText("Test", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "Revoke" }).click();
  const strip = admin.getByRole("group", { name: "Revoke" });
  await expect(strip).toContainText("The test licence's other computers keep working");
  await strip.getByRole("button", { name: "Revoke" }).click();
  await expect(row).toHaveCount(0); // the list shows computers in use
  await expect(admin.getByRole("row").filter({ hasText: second.name })).toHaveCount(1);

  const after = await api.get<{ active_computers: number; state: string }>(`/licences/${licence.id}`);
  expect([after.state, after.active_computers]).toEqual(["redeemed", 1]);
});
