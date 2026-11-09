// Updates: the Overview's Server section in each state, the notice on every page, Check now, and
// the Update button with its confirmation. The specs stand in for the update scripts by writing
// the files they would write in the server's data folder; every test removes them again.

import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, test, type AdminApi } from "./fixtures";
import { SERVERS } from "./servers";

const DATA = SERVERS.flows.dataDir;
const UPDATES = join(DATA, "updates");
const STATUS = join(DATA, "clavren-update", "status.json");
const INSTALLER = join(DATA, "clavren-update.sh");

type Updates = { running_version: string };

function newer(running: string): string {
  const [major, minor] = running.split(".").map(Number);
  return `${major}.${(minor ?? 0) + 1}.0`;
}

function writeLatest(version: string, changes: Record<string, unknown> = {}): void {
  mkdirSync(UPDATES, { recursive: true });
  const latest = {
    checked_at: "2030-01-15T14:02:00Z",
    version,
    release_url: `https://github.com/Ran-n/clavren/releases/tag/v${version}`,
    published_at: "2030-01-15T10:00:00Z",
    installable: true,
    bundle_url: null,
    checksum_url: null,
    backup_drive_mounted: true,
    error: null,
    ...changes,
  };
  writeFileSync(join(UPDATES, "latest.json"), JSON.stringify(latest));
}

function writeStatus(status: Record<string, unknown>): void {
  mkdirSync(join(DATA, "clavren-update"), { recursive: true });
  writeFileSync(STATUS, JSON.stringify(status));
}

function clean(): void {
  for (const name of ["latest.json", "check-request", "request.json"]) rmSync(join(UPDATES, name), { force: true });
  rmSync(STATUS, { force: true });
  rmSync(INSTALLER, { force: true });
}

async function running(api: AdminApi): Promise<string> {
  return (await api.get<Updates>("/updates")).running_version;
}

const notice = (page: Page) => page.getByRole("main").locator(".notice").first();

test.describe.configure({ mode: "serial" });
test.beforeEach(clean);
test.afterEach(clean);

test("before any check the Overview shows the version and that it wasn't checked", async ({ admin, api }) => {
  await admin.goto("/");
  await expect(admin.getByText(`Version ${await running(api)}`)).toBeVisible();
  await expect(admin.getByText("Not checked yet.")).toBeVisible();
  await expect(admin.getByText(/is available/)).toHaveCount(0);
});

test("a newer release shows on every page and on the Overview", async ({ admin, api }) => {
  const version = newer(await running(api));
  writeLatest(version);
  for (const path of ["/licences", "/audit", "/"]) {
    await admin.goto(path);
    await expect(notice(admin)).toContainText(`Clavren ${version} is available (this server runs ${await running(api)}).`);
    const link = notice(admin).getByRole("link", { name: "What's new" });
    await expect(link).toHaveAttribute("href", `https://github.com/Ran-n/clavren/releases/tag/v${version}`);
    await expect(link).toHaveAttribute("rel", "noreferrer");
    await expect(link).toHaveAttribute("target", "_blank");
  }
  await expect(admin.getByText(`Last checked 15 Jan 2030, 14:02. Clavren ${version} is available.`)).toBeVisible();
  // a source installation has no updater: no Update button
  await expect(admin.getByRole("button", { name: /^Update to/ })).toHaveCount(0);
});

test("a failed check says why, and a release still being published says so", async ({ admin, api }) => {
  const version = newer(await running(api));
  writeLatest(version, { error: "could not reach api.github.com", installable: false });
  await admin.goto("/");
  await expect(admin.getByText("Last check failed at 15 Jan 2030, 14:02: could not reach api.github.com.")).toBeVisible();
  await expect(notice(admin)).toContainText(`Clavren ${version} is being published; it can be installed once its files are uploaded.`);
});

test("Check now asks for a check and shows its result", async ({ admin, api }) => {
  const version = newer(await running(api));
  writeLatest(await running(api), { checked_at: "2030-01-15T08:00:00Z" });
  await admin.goto("/");
  await expect(admin.getByText(/Last checked 15 Jan 2030, 08:00\. This is the newest release\./)).toBeVisible();
  await admin.getByRole("button", { name: "Check now" }).click();
  await expect.poll(() => existsSync(join(UPDATES, "check-request"))).toBe(true);
  // the check script answers the request
  rmSync(join(UPDATES, "check-request"));
  writeLatest(version, { checked_at: "2030-01-15T09:30:00Z" });
  await expect(admin.getByText(`Last checked 15 Jan 2030, 09:30. Clavren ${version} is available.`)).toBeVisible();
  await expect(notice(admin)).toContainText(`Clavren ${version} is available`);
});

test("Update asks first, needs the box ticked without a backup drive, and shows the progress", async ({ admin, api }) => {
  const version = newer(await running(api));
  writeFileSync(INSTALLER, "#!/bin/sh\n");
  writeLatest(version, { backup_drive_mounted: false });
  await admin.goto("/licences");
  const update = admin.getByRole("button", { name: `Update to ${version}` });
  await update.click();
  const strip = admin.getByRole("group", { name: "Confirm" });
  await expect(strip).toContainText(`Clavren installs ${version} and restarts.`);
  await admin.keyboard.press("Escape");
  await expect(strip).toHaveCount(0);

  await update.click();
  const confirm = strip.getByRole("button", { name: "Confirm" });
  await expect(confirm).toBeDisabled();
  expect(Number(await confirm.evaluate((element) => getComputedStyle(element).opacity))).toBeLessThan(0.6);
  await strip.getByLabel("Update without a backup (the backup drive isn't mounted)").check();
  await confirm.click();
  await expect(notice(admin)).toContainText(`Updating to ${version}…`);
  const request = JSON.parse(readFileSync(join(UPDATES, "request.json"), "utf8")) as unknown;
  expect(request).toEqual({ version, skip_backup: true });

  // the updater takes the request and fails: the reason shows, with the button again
  rmSync(join(UPDATES, "request.json"));
  writeStatus({
    version,
    state: "failed",
    stage: "checking",
    message: "The download didn't match its checksum, so nothing was installed.",
    started_at: "2030-01-15T14:05:00Z",
    finished_at: "2030-01-15T14:06:00Z",
  });
  await expect(notice(admin)).toContainText("The update failed: The download didn't match its checksum, so nothing was installed.");
  await expect(update).toBeEnabled();

  await admin.goto("/audit");
  await expect(admin.getByRole("row").nth(1)).toContainText(`Server update requested: ${await running(api)} to ${version}, without a backup`);
});

test("with a backup drive, Confirm needs no box", async ({ admin, api }) => {
  const version = newer(await running(api));
  writeFileSync(INSTALLER, "#!/bin/sh\n");
  writeLatest(version);
  await admin.goto("/");
  await admin.getByRole("button", { name: `Update to ${version}` }).click();
  const strip = admin.getByRole("group", { name: "Confirm" });
  await expect(strip).toContainText(`Clavren backs up, installs ${version} and restarts.`);
  await expect(strip.getByRole("checkbox")).toHaveCount(0);
  await expect(strip.getByRole("button", { name: "Confirm" })).toBeEnabled();
  await strip.getByRole("button", { name: "Cancel" }).click();
  await expect(strip).toHaveCount(0);
});

test("a release not uploaded yet can't be installed", async ({ admin, api }) => {
  writeFileSync(INSTALLER, "#!/bin/sh\n");
  writeLatest(newer(await running(api)), { installable: false });
  await admin.goto("/");
  await expect(admin.getByRole("button", { name: /^Update to/ })).toBeDisabled();
});

test("once the server runs the new version the page reloads once and says so", async ({ admin, api }) => {
  const from = await running(api);
  const version = newer(from);
  writeFileSync(INSTALLER, "#!/bin/sh\n");
  writeLatest(version);
  const started = "2030-01-15T14:05:00Z";
  writeStatus({ version, state: "running", stage: "installing", message: null, started_at: started, finished_at: null });
  // The server can't change its own version in a test: after two answers, report the new one.
  let answers = 0;
  let reloads = 0;
  admin.on("load", () => reloads++);
  await admin.route("**/api/admin/updates", async (route) => {
    const response = await route.fetch();
    const body = (await response.json()) as Record<string, unknown>;
    answers++;
    if (answers > 2) Object.assign(body, { running_version: version, available: null, update: null, can_install: false });
    await route.fulfill({ response, json: body });
  });
  await admin.goto("/");
  await expect(notice(admin)).toContainText(`Updating to ${version}… (installing)`);
  await expect(admin.getByText(`Updated to ${version}.`)).toBeVisible({ timeout: 20_000 });
  expect(reloads).toBe(2); // the first load and one reload
  await expect(admin.getByText(`Version ${version}`)).toBeVisible();
});
