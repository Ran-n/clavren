// The Activations page (revoking a computer resets its licence; computers in use, ended or all),
// the audit log's filters (action, admin, days), and the Overview.

import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, test } from "./fixtures";

test("revoking a computer on the Activations page frees its licence", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const customer = unique("Eva");
  const issued = (await issue(api, product.id, { customer: { name: customer } })).licences[0]!;
  const device = unique("EVA-PC");
  await new Device(device).activate(request, product.code, issued.key, "203.0.113.31");

  await admin.goto("/activations");
  await admin.getByLabel("Search by computer, platform or version").fill(device);
  const row = admin.getByRole("row").filter({ hasText: device });
  await expect(row).toContainText(product.name);
  await expect(row).toContainText(customer);
  await row.getByRole("button", { name: "Revoke" }).click();
  await admin.getByRole("group", { name: "Revoke" }).getByRole("button", { name: "Revoke" }).click();
  await expect(row).toHaveCount(0); // the list shows computers in use
  expect((await licence(api, issued.id)).state).toBe("unused");
});

test("the audit log names who did what, and filters by action and admin", async ({ admin, api, credentials }) => {
  const product = await createProduct(api);
  await issue(api, product.id);
  await admin.goto("/audit");
  await admin.getByLabel("Action").selectOption({ label: "Licences" });
  const first = admin.getByRole("row").nth(1);
  await expect(first).toContainText("Licence issued");
  await first.getByRole("button", { name: credentials.info.username }).click();
  await expect(admin.getByText("Showing one admin.")).toBeVisible();
  await admin.getByRole("button", { name: "Show everything" }).click();
  await expect(admin.getByText("Showing one admin.")).toBeHidden();
});

test("the Overview counts licences and warns that there is no backup", async ({ admin }) => {
  await admin.goto("/");
  await expect(admin.getByRole("link", { name: /Unused/ })).toBeVisible();
  await expect(admin.getByText("There is no backup yet.")).toBeVisible();
  await admin.getByRole("link", { name: /Unused/ }).click();
  await expect(admin).toHaveURL(/\/licences$/);
  await expect(admin.getByLabel("State")).toHaveValue("unused");
});

test("the Activations page shows computers in use, the ended ones, or all", async ({ admin, api, request }) => {
  const product = await createProduct(api);
  const issued = (await issue(api, product.id)).licences[0]!;
  const device = unique("GONE-PC");
  await new Device(device).activate(request, product.code, issued.key, "203.0.113.32");
  await api.call("POST", `/licences/${issued.id}/reset`, {});

  await admin.goto("/activations");
  const state = admin.getByLabel("State");
  const search = admin.getByLabel("Search by computer, platform or version");
  try {
    await search.fill(device);
    await expect(admin.getByText("No computer matches.")).toBeVisible(); // in use: this one isn't
    await state.selectOption({ label: "Ended" });
    const row = admin.getByRole("row").filter({ hasText: device });
    await expect(row).toContainText("Reset");
    await expect(row.getByRole("button", { name: "Revoke" })).toBeDisabled();
    await state.selectOption({ label: "All" });
    await expect(row).toHaveCount(1);
  } finally {
    // the filter and the search are remembered: the other tests start from the list as it opens
    await state.selectOption({ label: "In use" });
    await search.fill("");
    await expect(admin.getByRole("row").filter({ hasText: device })).toHaveCount(0);
  }
});

test("the audit log shows the days between From and To", async ({ admin, api }) => {
  const product = await createProduct(api);
  await issue(api, product.id);
  await admin.goto("/audit");
  const [from, to] = [admin.getByLabel("From", { exact: true }), admin.getByLabel("To", { exact: true })];
  const today = "2027-03-01"; // the test server's clock stands still on this day
  try {
    await admin.getByLabel("Action").selectOption({ label: "Licences" });
    await from.fill("2099-01-01");
    await expect(admin.getByText("Nothing recorded matches.")).toBeVisible();
    await from.fill(today);
    await to.fill(today); // To is the whole day
    await expect(admin.getByRole("row").nth(1)).toContainText("Licence issued");
    await from.fill("");
    await to.fill("2000-01-01");
    await expect(admin.getByText("Nothing recorded matches.")).toBeVisible();
  } finally {
    // remembered filters: the other tests start from the whole log
    await from.fill("");
    await to.fill("");
    await admin.getByLabel("Action").selectOption({ index: 0 });
    await expect(admin.getByText("Nothing recorded matches.")).toBeHidden();
  }
});
