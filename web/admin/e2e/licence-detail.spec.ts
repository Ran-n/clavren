// One licence: Reset only while in use (dimmed with a reason otherwise), the inline confirmation,
// Revoke with a reason, Copy message, and the computers it was on. Checked through the API.

import { createProduct, issue, licence, unique } from "./data";
import { Device, expect, test } from "./fixtures";

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
  await expect(strip).toContainText("locks at its next check");
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
