// One licence: Reset only while in use (dimmed with a reason otherwise), the inline confirmation,
// Revoke with a reason, Delete with a warning for its state, Copy message, the computers it was
// on, and its customer and revocation reason edited in place. Checked through the API.

import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, test } from "./fixtures";

/** The toolbar's Product filter (the column picker has a "Product" checkbox too). */
const productFilter = (page: import("@playwright/test").Page) => page.getByRole("combobox", { name: /^Product/ });

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

test("reset frees a licence in use, revoke ends it for good", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const customer = unique("Carla");
  const issued = (await issue(api, product.id, { customer: { name: customer } })).licences[0]!;
  await admin.goto(`/licences/${issued.id}`);

  const reset = admin.getByRole("button", { name: "Reset" });
  await expect(reset).toBeDisabled();
  await expect(admin.locator(".button-reason").filter({ has: reset })).toHaveAttribute("title", "This licence isn't in use on any computer");
  expect(Number(await reset.evaluate((button) => getComputedStyle(button).opacity))).toBeLessThan(0.6);
  await expect(admin.getByText("Not activated on any computer yet.")).toBeVisible();

  await admin.getByRole("button", { name: "Copy message" }).click();
  await expect.poll(() => admin.evaluate(() => navigator.clipboard.readText())).toContain(issued.key);

  await new Device("CARLA-PC").activate(request, product.code, issued.key, "203.0.113.21");
  await admin.reload();
  await expect(admin.getByRole("heading", { name: /Licence/ })).toContainText("In use");
  await expect(admin.getByRole("table", { name: "Computers" })).toContainText("CARLA-PC");

  await admin.getByRole("button", { name: "Reset" }).click();
  const strip = admin.getByRole("group", { name: "Reset" });
  await expect(strip).toContainText("locks at its next check. If it is offline, at the latest in 30 days.");
  await strip.press("Escape");
  await expect(strip).toBeHidden();
  await admin.getByRole("button", { name: "Reset" }).click();
  await admin.getByRole("group", { name: "Reset" }).getByRole("button", { name: "Reset" }).click();
  await expect(admin.getByRole("heading", { name: /Licence/ })).toContainText("Unused");
  await expect(admin.getByRole("table", { name: "Computers" })).toContainText("Reset");
  expect((await licence(api, issued.id)).state).toBe("unused");

  await admin.getByRole("button", { name: "Revoke" }).click();
  await admin.getByLabel("Reason (optional)").fill("Chargeback");
  await admin.getByRole("group", { name: "Revoke" }).getByRole("button", { name: "Revoke" }).click();
  await expect(admin.getByRole("heading", { name: /Licence/ })).toContainText("Revoked");
  await expect(admin.getByText("Deleted when revoked")).toBeVisible();
  await expect(admin.getByRole("button", { name: "Revoke" })).toBeDisabled();
  const revoked = await licence(api, issued.id);
  expect([revoked.state, revoked.revocation_reason]).toEqual(["revoked", "Chargeback"]);
});

test("the customer is added, changed and removed in place", async ({ admin, api }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id)).licences[0]!;
  const existing = unique("Eva Existing");
  const existingEmail = `${existing.replace(/\W/g, "").toLowerCase()}@example.com`;
  await api.call("POST", "/customers", { name: existing, email: existingEmail });
  let patches = 0;
  admin.on("request", (request) => {
    if (request.method() === "PATCH") patches += 1;
  });
  await admin.goto(`/licences/${issued.id}`);
  const editor = admin.getByRole("group", { name: "Customer" });
  const field = editor.getByRole("combobox", { name: "Customer (optional)" });

  await admin.getByRole("button", { name: "Add customer" }).click();
  const save = editor.getByRole("button", { name: "Save" });
  await expect(save).toBeDisabled();
  await expect(editor.locator(".button-reason").filter({ hasText: "Save" })).toHaveAttribute("title", "Pick or type a customer");
  await field.click();
  await field.press("Escape"); // closes the list
  await expect(editor).toBeVisible();
  await field.press("Escape"); // cancels
  await expect(editor).toBeHidden();
  expect(patches, "Escape sends nothing").toBe(0);

  const added = unique("Nora New");
  await admin.getByRole("button", { name: "Add customer" }).click();
  await field.fill(added);
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(editor).toBeHidden();
  await expect(admin.getByText(added)).toBeVisible();
  await admin.getByRole("button", { name: "Copy message" }).click();
  await expect.poll(() => admin.evaluate(() => navigator.clipboard.readText())).toContain(`Hello ${added}`);

  await admin.getByRole("button", { name: "Change" }).click();
  await field.fill(unique("Taken"));
  await editor.getByLabel("New customer's email (optional)").fill(existingEmail);
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(editor.getByRole("alert")).toContainText("Another customer already has this email address.");
  await expect(editor.getByLabel("New customer's email (optional)"), "what was typed stays").toHaveValue(existingEmail);
  await editor.getByRole("button", { name: "Cancel" }).click();

  await admin.getByRole("button", { name: "Change" }).click();
  await field.fill(existing);
  await editor.getByRole("option", { name: new RegExp(existing) }).click();
  await editor.getByRole("button", { name: "Save" }).click();
  await expect(admin.getByText(`${existing} · ${existingEmail}`)).toBeVisible();
  const linked = await api.get<{ customer: string | null }>(`/licences/${issued.id}`);
  expect(linked.customer).not.toBeNull();

  await admin.getByRole("button", { name: "Change" }).click();
  await editor.getByRole("button", { name: "Remove customer" }).click();
  await expect(admin.getByText("No customer")).toBeVisible();
  expect((await api.get<{ customer: string | null }>(`/licences/${issued.id}`)).customer).toBeNull();
  await api.get(`/customers/${linked.customer}`); // still there: removing only unlinks
});

test("a revoked licence's reason is edited in place; a live one has none to edit", async ({ admin, api }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id)).licences[0]!;
  await admin.goto(`/licences/${issued.id}`);
  await expect(admin.getByRole("button", { name: "Edit revocation reason" })).toHaveCount(0);
  await api.call("POST", `/licences/${issued.id}/revoke`, { reason: "dup" });
  await admin.reload();
  await admin.getByRole("button", { name: "Edit revocation reason" }).click();
  await admin.getByLabel("revocation reason").fill("Duplicate order");
  await admin.getByLabel("revocation reason").press("Enter");
  await expect(admin.getByRole("button", { name: "Edit revocation reason" })).toHaveText("Duplicate order");
  await admin.reload();
  await expect(admin.getByRole("button", { name: "Edit revocation reason" })).toHaveText("Duplicate order");
  expect((await licence(api, issued.id)).revocation_reason).toBe("Duplicate order");
});

test("a licence is deleted in any state, each with its own warning", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const [unused, inUse, revoked] = (await issue(api, product.id, { quantity: 3 })).licences;
  await new Device("DELETE-PC").activate(request, product.code, inUse!.key, "203.0.113.41");
  await api.call("POST", `/licences/${revoked!.id}/revoke`, {});
  const warnings: [typeof unused, string][] = [
    [unused, "Its key stops working and the licence disappears."],
    [inUse, "The computer using it locks at its next check"],
    [revoked, "It disappears with the computers it was used on."],
  ];
  let deletes = 0;
  admin.on("request", (request) => {
    if (request.method() === "DELETE") deletes += 1;
  });

  for (const [one, warning] of warnings) {
    await admin.goto(`/licences/${one!.id}`);
    await admin.getByRole("button", { name: "Delete" }).click();
    const strip = admin.getByRole("group", { name: "Delete" });
    await expect(strip).toContainText(warning);
    await expect(strip).toContainText("This can't be undone.");
    if (one === unused) {
      await strip.press("Escape");
      await expect(strip).toBeHidden();
      expect(deletes, "Escape sends nothing").toBe(0);
      await admin.getByRole("button", { name: "Delete" }).click();
    }
    await admin.getByRole("group", { name: "Delete" }).getByRole("button", { name: "Delete" }).click();
    await expect(admin).toHaveURL(/\/licences$/);
    await expect(admin.getByText("Licence deleted.")).toBeVisible();
    const gone = await api.call<unknown>("GET", `/licences/${one!.id}`).then(
      () => false,
      () => true,
    );
    expect(gone).toBe(true);
  }
  await productFilter(admin).selectOption({ label: product.name });
  await expect(admin.getByText("No licence matches.")).toBeVisible();
});

test("the Revoke strip says when an offline computer locks at the latest, from its product's limit", async ({ admin, api }) => {
  const product = await createProduct(api);
  await api.call("PATCH", `/products/${product.id}`, { offline_days: 7 });
  const issued = (await issue(api, product.id)).licences[0]!;
  await admin.goto(`/licences/${issued.id}`);
  await admin.getByRole("button", { name: "Revoke" }).click();
  await expect(admin.getByRole("group", { name: "Revoke" })).toContainText("locks at its next check. If it is offline, at the latest in 7 days.");
});
