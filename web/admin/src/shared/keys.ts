// Licence keys in the browser: only enough to recognise one, so it is never saved in preferences.

const KEY_CHARACTERS = /^(BRN)?[0-9A-Z]{16}$/;

/** Whether `text` could be a licence key typed in any form (case, spaces, dashes). */
export function looksLikeKey(text: string): boolean {
  return KEY_CHARACTERS.test(text.replace(/[\s-]/g, "").toUpperCase());
}

export const HIDDEN_KEY = "BRN-••••-••••-••••-••••";
