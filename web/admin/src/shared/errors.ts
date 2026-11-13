// Every error in plain English, with what to do about it. Codes, statuses and server details
// never reach the screen.

import { ApiError } from "./api";
import { formatNumber } from "./format";

const UNEXPECTED = "Something unexpected happened. Reload the page and try again.";

const MESSAGES: Record<string, string> = {
  network: "Clavren can't be reached. Check your connection and try again.",
  internal_error: "Something went wrong on the server. Try again; if it keeps happening, check the server log.",
  unauthorized: "Your session has ended. Sign in again.",
  csrf_failed: "This page is out of date. Reload it and try again.",
  session_changed: "You signed in as another admin in another tab. This page now shows that account; try again.",
  invalid_credentials: "The username, password or code is wrong.",
  weak_password: "The new password must be 12 to 1,024 characters long.",
  not_found: "It isn't there any more; it may have been deleted. Reload the page.",
  product_code_taken: "Another product already uses this code. Choose a different one.",
  product_code_locked: "This product has licences, so its code can't change: their tokens carry it.",
  product_inactive: "This product is inactive. Activate it on the Products page before issuing licences.",
  customer_email_taken: "Another customer already has this email address.",
  licence_revoked: "This licence is revoked, and that can't be undone.",
  licence_not_in_use: "This licence isn't in use on any computer, so there is nothing to reset.",
  licence_not_revoked: "Only a revoked licence has a revocation reason.",
  activation_revoked: "This computer's activation has already ended.",
  application_decided: "This application has already been approved or rejected. Reload the page to see how.",
  updates_off: "Checking for updates is off on this server (CLAVREN_UPDATE_SOURCE=off).",
  update_file_unreadable:
    "The update check left a file Clavren can't read. On the server, run sudo systemctl start clavren-update-check.",
  update_not_available: "That release can't be installed any more. Reload the page to see the newest one.",
  update_running: "An update is running already. Wait for it to finish.",
  backup_required: "The backup drive isn't mounted. Tick \"Update without a backup\" to update anyway, or mount it first.",
  updates_unsupported: "This server was installed from source, so it can't update itself. Upgrade it by hand (deployment.md).",
};

export function plural(count: number, word: string): string {
  return `${formatNumber(count)} ${word}${count === 1 ? "" : "s"}`;
}

/** After "locks at its next check": when an offline computer locks at the latest, from its
 * product's offline limit (empty when the product isn't known). */
export function offlineLimit(product: { offline_days: number } | null | undefined): string {
  return product ? ` If it is offline, at the latest in ${plural(product.offline_days, "day")}.` : "";
}

/** Before deleting licences: how many computers lock, as a sentence (empty when none). */
export function inUseWarning(inUse: number): string {
  if (inUse === 0) return "";
  if (inUse === 1) return "1 is in use: that computer locks at its next check. ";
  return `${formatNumber(inUse)} are in use: those computers lock at their next check. `;
}

/** What a form says instead of the usual message for some codes (e.g. which password is wrong). */
export type Wording = Partial<Record<string, string>>;

/** `item` names what a bulk error's IDs are, for the count it adds. */
export function describe(error: unknown, wording: Wording = {}, item = "licence"): string {
  if (!(error instanceof ApiError)) return UNEXPECTED;
  if (error.code === "rate_limited") {
    const minutes = Math.max(1, Math.ceil((error.retryAfterSeconds ?? 60) / 60));
    return `Too many failed attempts. Wait ${plural(minutes, "minute")} and try again.`;
  }
  if (error.code === "invalid_request" && error.message) {
    // These name the field and what is wrong with it, in plain words.
    return `${error.message.charAt(0).toUpperCase()}${error.message.slice(1)}.`;
  }
  const message = wording[error.code] ?? MESSAGES[error.code] ?? UNEXPECTED;
  return error.ids.length > 1 ? `${message} (${plural(error.ids.length, item)} in the selection.)` : message;
}
