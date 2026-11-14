// Helpers shared by the preferences specs: the theme button, and the settings as the server saves them.

import { expect, type Page, type Request } from "@playwright/test";

export const themeButton = (page: Page) => page.getByRole("button", { name: /^Theme:/ });
export const savedPreferences = (page: Page) =>
  page.waitForResponse((response) => response.url().endsWith("/api/admin/preferences") && response.request().method() === "PUT");

/** Runs the dashboard's delayed save now and checks that it sends nothing: a save starts by
 * reading the saved settings, so any request about them counts. The page's clock must have been
 * installed (`page.clock.install()`) before it loaded. */
export async function savesNothing(page: Page): Promise<void> {
  const asked: string[] = [];
  const note = (request: Request) => {
    if (request.url().endsWith("/api/admin/preferences")) asked.push(request.method());
  };
  page.on("request", note);
  await page.clock.fastForward(2_000); // well past the save delay
  await page.evaluate(() => undefined); // whatever the timers sent has been seen by now
  page.off("request", note);
  expect(asked, "nothing about the settings is sent").toEqual([]);
}
