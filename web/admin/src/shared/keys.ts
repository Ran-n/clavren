// Licence keys in the browser: only enough to recognise one, so it is never saved in preferences.

// The key alphabet (Crockford Base32, which has no U; I, L and O are read as 1 and 0), with the
// prefix optional: the 16 characters after it are the secret part.
const KEY_CHARACTERS = /^(BRN)?[0-9A-TV-Z]{16}$/;

/** Whether `text` could be a licence key typed in any form (case, spaces, dashes). */
export function looksLikeKey(text: string): boolean {
  return KEY_CHARACTERS.test(text.replace(/[\s-]/g, "").toUpperCase());
}

export const HIDDEN_KEY = "BRN-••••-••••-••••-••••";
