// End-to-end and rendered tests of the dashboard against a real Clavren server
// (crates/clavren-server/examples/e2e_server.rs), in headless Chromium. Nothing here touches the
// real mouse or keyboard. `npm run e2e`; `npm run e2e -- --update-snapshots` after reviewing a
// deliberate visual change.

import { defineConfig, devices } from "@playwright/test";
import { SERVERS } from "./e2e/servers";

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  // One worker: the specs share one server and one admin's sign-in codes.
  workers: 1,
  fullyParallel: false,
  timeout: 120_000, // generous: the suite also runs on a busy machine
  expect: { timeout: 10_000, toHaveScreenshot: { maxDiffPixels: 20, animations: "disabled" } },
  reporter: [["list"]],
  snapshotPathTemplate: "{testDir}/baselines/{arg}{ext}",
  use: {
    ...devices["Desktop Chrome"],
    headless: true,
    locale: "en-GB",
    timezoneId: "UTC",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "flows", testIgnore: /rendered\.spec\.ts/, use: { baseURL: SERVERS.flows.url } },
    { name: "rendered", testMatch: /rendered\.spec\.ts/, use: { baseURL: SERVERS.rendered.url } },
  ],
});
