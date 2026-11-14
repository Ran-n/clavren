// The theme on this browser: applied as soon as it is chosen, and kept here (localStorage) so the
// last one shows before the saved settings arrive.

import type { Theme } from "./preferenceTypes";

/** This browser's copy of the last theme, applied before the saved settings arrive. */
const THEME_KEY = "clavren-theme";

export function applyTheme(theme: Theme): void {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // storage blocked: the theme still applies, from the saved settings, a moment later
  }
}

/** The theme this browser used last, so the page doesn't flash another one while loading. */
export function applyLastTheme(): void {
  let last: string | null = null;
  try {
    last = localStorage.getItem(THEME_KEY);
  } catch {
    return; // storage blocked: the system theme until the saved settings arrive
  }
  if (last === "light" || last === "dark" || last === "system") applyTheme(last);
}
