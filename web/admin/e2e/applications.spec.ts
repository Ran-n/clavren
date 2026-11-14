// Licence applications from the web form to the licence: an application sent on the web form, approved
// on the Applications page (its customer filled in from it) and its key collected on the status page;
// a rejection's reason reaches the applicant; and an approved application whose licence was revoked
// reads as withdrawn. Results are checked through the API.

import { unique } from "./data";
import { expect, test } from "./fixtures";
import { apply, openProduct, ownAddress, type Listed } from "./applications-helpers";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

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

test("an approved application whose licence was revoked since reads as withdrawn", async ({ api, browser }) => {
  const product = await openProduct(api);
  const name = unique("Hal");
  const applicant = await browser.newPage();
  await ownAddress(applicant);
  await apply(applicant, product.code, name, `${name.toLowerCase()}@example.com`);
  const { data } = await api.get<Listed>(`/applications?product=${product.id}&state=pending`);
  const approved = await api.call<{ licence: string }>("POST", `/applications/${data[0]!.id}/approve`, {});
  await api.call("POST", `/licences/${approved.licence}/revoke`, {});

  await applicant.getByRole("button", { name: "Check again" }).click();
  await expect(applicant.getByText("its licence has been withdrawn since")).toBeVisible();
  await expect(applicant.locator("code.key")).toHaveCount(0);
  await applicant.close();
});
