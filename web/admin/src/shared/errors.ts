// Every error in plain English, with what to do about it. Codes, statuses and server details
// never reach the screen.

import { ApiError } from "./api";

const UNEXPECTED = "Something unexpected happened. Reload the page and try again.";

const MESSAGES: Record<string, string> = {
  network: "Clavren can't be reached. Check your connection and try again.",
  internal_error: "Something went wrong on the server. Try again; if it keeps happening, check the server log.",
  unauthorized: "Your session has ended. Sign in again.",
  csrf_failed: "This page is out of date. Reload it and try again.",
  invalid_credentials: "The username, password or code is wrong.",
  weak_password: "The new password is too short: use at least 12 characters.",
  not_found: "It isn't there any more; it may have been deleted. Reload the page.",
  product_code_taken: "Another product already uses this code. Choose a different one.",
  product_code_locked: "This product has licences, so its code can't change: their tokens carry it.",
  product_inactive: "This product is inactive. Activate it on the Products page before issuing licences.",
  customer_email_taken: "Another customer already has this email address.",
  licence_revoked: "This licence is revoked, and that can't be undone.",
  licence_not_in_use: "This licence isn't in use on any computer, so there is nothing to reset.",
  licence_not_revoked: "Only a revoked licence has a revocation reason.",
  activation_revoked: "This computer's activation has already ended.",
};

export function plural(count: number, word: string): string {
  return `${count.toLocaleString()} ${word}${count === 1 ? "" : "s"}`;
}

/** Before deleting licences: how many computers lock, as a sentence (empty when none). */
export function inUseWarning(inUse: number): string {
  if (inUse === 0) return "";
  if (inUse === 1) return "1 is in use: that computer locks at its next check. ";
  return `${inUse.toLocaleString()} are in use: those computers lock at their next check. `;
}

export function describe(error: unknown): string {
  if (!(error instanceof ApiError)) return UNEXPECTED;
  if (error.code === "rate_limited") {
    const minutes = Math.max(1, Math.ceil((error.retryAfterSeconds ?? 60) / 60));
    return `Too many failed attempts. Wait ${plural(minutes, "minute")} and try again.`;
  }
  if (error.code === "invalid_request" && error.message) {
    // These name the field and what is wrong with it, in plain words.
    return `${error.message.charAt(0).toUpperCase()}${error.message.slice(1)}.`;
  }
  const message = MESSAGES[error.code] ?? UNEXPECTED;
  return error.ids.length > 1 ? `${message} (${plural(error.ids.length, "licence")} in the selection.)` : message;
}
