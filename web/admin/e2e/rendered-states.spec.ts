// Reviewed baseline screenshots of the states that open in place, light and dark, at desktop and phone
// width: the issue panel with the list of every customer, and with its keys just issued; a licence's
// customer being changed; a licence and a product being deleted; a product's offline days being
// lowered; a test licence being reset; a new product with its test licence; an application being
// approved; the theme button hovered; and an update confirmed without a backup drive, and running. The
// same fixed data and keys rules as rendered.spec.ts apply.

import { expect, test } from "./fixtures";
import { answerUpdates, arrange, capture, LICENCE_ID, NO_UPDATE, type Seeded } from "./rendered-helpers";

test.use({ server: "rendered" });

/** What creating a product answers in the capture of its test licence: fixed, so no key changes a baseline. */
const CREATED_PRODUCT = {
  id: "00000000-0000-7000-8000-000000000001",
  code: "avalio-lab",
  name: "Avalio Lab",
  description: null,
  key_instructions: null,
  offline_days: 30,
  price: null,
  message_template: "",
  is_active: true,
  created_at: "2027-03-01T09:00:00Z",
  updated_at: "2027-03-01T09:00:00Z",
  test_licence: { id: "00000000-0000-7000-8000-000000000002", key: "BRN-TEST-0000-0000-0000" },
};

/** What issuing three keys answers in the capture of the issued keys: fixed, so no key changes a baseline. */
const ISSUED_THREE = {
  licences: ["BRN-7KPX-4M9D-QW2R-8NFC", "BRN-3HTV-9C2M-XK4P-6WQD", "BRN-5RNE-8F7T-JM3K-2YBH"].map((key, index) => ({
    id: `00000000-0000-7000-8000-00000000001${index}`,
    key,
    is_test: false,
  })),
  customer: null,
  message: null,
};

const UPDATE_AVAILABLE = {
  ...NO_UPDATE,
  available: { version: "1.1.0", release_url: "https://github.com/Ran-n/clavren/releases/tag/v1.1.0", published_at: "2027-02-28T10:00:00Z", installable: true },
  backup_drive_mounted: false,
  can_install: true,
};
const UPDATE_RUNNING = {
  ...UPDATE_AVAILABLE,
  update: { version: "1.1.0", state: "running", stage: "backing_up", message: null, started_at: "2027-03-01T09:29:00Z", finished_at: null },
  can_install: false,
};

test.describe("every page, light and dark, desktop and phone", () => {
  let seeded: Seeded;
  test.beforeEach(async ({ api, request, page }) => {
    seeded = await arrange(api, request, page);
  });

  test("issuing, with the list of every customer open", async ({ admin }) =>
    capture(admin, "issue-customers", "/licences", undefined, async (page) => {
      // the panel is remembered open after the first size and scheme
      const open = page.getByRole("button", { name: "Issue licences" });
      if (await open.isVisible()) await open.click();
      await page.getByRole("combobox", { name: "Customer (optional)" }).focus();
      await expect(page.getByRole("option", { name: /Carla Gómez/ })).toBeVisible();
    }));
  test("three keys just issued, on the clipboard, each with Copy", async ({ admin }) => {
    await admin.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await admin.route("**/api/admin/licences", (route) =>
      route.request().method() === "POST" ? route.fulfill({ status: 201, json: ISSUED_THREE }) : route.fallback(),
    );
    await capture(admin, "issued-keys", "/licences", undefined, async (page) => {
      const open = page.getByRole("button", { name: "Issue licences" });
      if (await open.isVisible()) await open.click();
      await page.getByRole("region", { name: "Issue licences" }).getByRole("button", { name: "Issue", exact: true }).click();
      await expect(page.getByText("The 3 keys are on the clipboard.")).toBeVisible();
    });
  });
  test("a licence's customer being changed", async ({ admin }) =>
    capture(admin, "licence-customer-editor", `/licences/${seeded.activeLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Change" }).click();
      await expect(page.getByRole("group", { name: "Customer" })).toBeVisible();
    }));
  test("deleting a licence in use", async ({ admin }) =>
    capture(admin, "licence-delete", `/licences/${seeded.activeLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Delete" }).click();
      await expect(page.getByRole("group", { name: "Delete" })).toContainText("locks at its next check");
    }));
  test("a product's offline days being lowered", async ({ admin }) =>
    capture(admin, "product-offline", "/products", undefined, async (page) => {
      const row = page.getByRole("row").filter({ hasText: "Avalio Lite" });
      await row.getByRole("button", { name: "Edit days offline" }).click();
      await row.getByLabel("days offline").fill("7");
      await row.getByLabel("days offline").press("Enter");
      await expect(page.getByRole("group", { name: "Limit" })).toBeVisible();
    }));
  test("deleting a product with licences", async ({ admin }) =>
    capture(admin, "product-delete", "/products", undefined, async (page) => {
      await page.getByRole("row").filter({ hasText: "Avalio Lite" }).getByRole("button", { name: "Delete" }).click();
      await expect(page.getByRole("group", { name: "Delete" })).toContainText("Type avalio-lite to confirm");
    }));
  test("a test licence on two computers, being reset", async ({ admin }) =>
    capture(admin, "licence-test-reset", `/licences/${seeded.testLicence}`, LICENCE_ID, async (page) => {
      await page.getByRole("button", { name: "Reset" }).click();
      await expect(page.getByRole("group", { name: "Reset" })).toContainText("Every computer using it locks");
    }));
  test("a new product with its test licence", async ({ admin }) => {
    // only the creation is answered here: the list is the real one, unchanged
    await admin.route("**/api/admin/products", (route) =>
      route.request().method() === "POST" ? route.fulfill({ status: 201, json: CREATED_PRODUCT }) : route.fallback(),
    );
    await capture(admin, "product-test-licence", "/products", undefined, async (page) => {
      await page.getByRole("button", { name: "New product" }).click();
      await page.getByLabel("New product's name").fill(CREATED_PRODUCT.name);
      await page.getByLabel("New product's name").press("Enter");
      await expect(page.getByText("Avalio Lab was created with a test licence")).toBeVisible();
    });
  });
  test("an application being approved", async ({ admin }) =>
    capture(admin, "application-approving", "/applications", () => [], async (page) => {
      await page.getByRole("row").filter({ hasText: "Dana Ruiz" }).getByRole("button", { name: "Approve" }).click();
      await expect(page.getByRole("combobox", { name: /^Customer/ })).toHaveValue("Dana Ruiz");
    }));
  test("the theme button hovered", async ({ admin }) =>
    capture(admin, "theme-button-hover", "/", undefined, async (page) => {
      await page.getByRole("button", { name: /^Theme:/ }).hover();
    }));
  test("an update being confirmed without a backup drive", async ({ admin }) => {
    await answerUpdates(admin, UPDATE_AVAILABLE);
    await capture(admin, "update-confirm", "/", undefined, async (page) => {
      await page.getByRole("button", { name: "Update to 1.1.0" }).click();
      await expect(page.getByRole("group", { name: "Confirm" })).toContainText("Update without a backup");
    });
    const confirm = admin.getByRole("group", { name: "Confirm" }).getByRole("button", { name: "Confirm" });
    await expect(confirm).toBeDisabled();
    expect(Number(await confirm.evaluate((element) => getComputedStyle(element).opacity))).toBeLessThan(0.6);
  });
  test("an update running", async ({ admin }) => {
    await answerUpdates(admin, UPDATE_RUNNING);
    await capture(admin, "update-running", "/licences", undefined, async (page) => {
      await expect(page.getByText("Updating to 1.1.0…")).toBeVisible();
    });
  });
});
