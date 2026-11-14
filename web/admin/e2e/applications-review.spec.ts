// The Applications page's list: selected applications deleted together; the Overview saying when
// applications are waiting; and the search by name or email, with the selected ones rejected together
// with one reason. Results are checked through the API.

import { expect, test } from "./fixtures";
import { Listed, openProduct } from "./applications-helpers";
import { unique } from "./data";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

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

test("applications are found by name or email, and the selected ones rejected together with one reason", async ({ admin, api, request }) => {
  const product = await openProduct(api);
  const names = [unique("Flo"), unique("Gus")];
  for (const [index, name] of names.entries()) {
    const response = await request.post("/api/v1/applications", {
      data: { product: product.code, name, email: `${name.toLowerCase()}@example.com` },
      headers: { "cf-connecting-ip": `198.51.100.${210 + index}` },
    });
    expect(response.status(), await response.text()).toBe(201);
  }
  await admin.goto("/applications");
  const search = admin.getByLabel("Search by name or email");
  const rows = admin.getByRole("row").filter({ hasText: /Flo-|Gus-/ });
  try {
    await admin.getByRole("combobox", { name: /^Product/ }).selectOption({ label: product.name });
    await expect(rows).toHaveCount(2);
    await search.fill(`${names[0]!.toLowerCase()}@example.com`);
    await expect(rows).toHaveCount(1);
    await expect(rows).toContainText(names[0]!);
    await search.fill("");
    await expect(rows).toHaveCount(2);

    for (const name of names) await admin.getByRole("row").filter({ hasText: name }).getByLabel("Select this row").check();
    await admin.getByRole("group", { name: "Selected applications" }).getByRole("button", { name: "Reject" }).click();
    const strip = admin.getByRole("group", { name: "Reject" });
    await expect(strip).toContainText("Reject 2 applications?");
    await strip.getByLabel("Reason (optional)").fill("Applications are closed");
    await strip.getByRole("button", { name: "Reject" }).click();
    await expect(admin.getByText("2 applications rejected.")).toBeVisible();
    await expect(rows).toHaveCount(0); // the list shows the waiting ones
  } finally {
    await search.fill(""); // the search is remembered
  }
  const { data } = await api.get<{ data: { rejection_reason: string | null }[] }>(`/applications?product=${product.id}&state=rejected`);
  expect(data.map((one) => one.rejection_reason)).toEqual(["Applications are closed", "Applications are closed"]);
});
