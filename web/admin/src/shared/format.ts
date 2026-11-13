// Dates, sizes and prices as people read them. Times are shown in the browser's time zone; expiry dates
// are whole days in UTC, as the server stores them.

/** The dashboard is in English only: every date and number is written the same way. */
export const LOCALE = "en-GB";

const dateTime = new Intl.DateTimeFormat(LOCALE, { dateStyle: "medium", timeStyle: "short" });
const date = new Intl.DateTimeFormat(LOCALE, { dateStyle: "medium", timeZone: "UTC" });
const number = new Intl.NumberFormat(LOCALE);

export function formatNumber(value: number): string {
  return number.format(value);
}

export function formatDateTime(value: string | null | undefined): string {
  return value ? dateTime.format(new Date(value)) : "—";
}

export function formatDate(value: string | null | undefined): string {
  return value ? date.format(new Date(value)) : "—";
}

/** `2027-07-01T23:59:59Z` → `2027-07-01`, for a date input. */
export function toDateInput(value: string | null | undefined): string {
  return value ? value.slice(0, 10) : "";
}

/** A date input's `2027-07-01` → the end of that day in UTC: the licence works all that day. */
export function fromDateInput(value: string): string | null {
  return value ? `${value}T23:59:59Z` : null;
}

/** The largest amount a price takes, in minor units (just under a million euros). */
export const MAX_PRICE_AMOUNT = 99_999_999;

/** How many decimals `currency`'s minor unit has (2 for EUR, 0 for JPY, 3 for KWD). */
export function currencyDigits(currency: string): number {
  return new Intl.NumberFormat("en", { style: "currency", currency }).resolvedOptions().maximumFractionDigits ?? 2;
}

/** `{amount: 1999, currency: "EUR"}` → `€19.99`. */
export function formatPrice(price: { amount: number; currency: string }): string {
  const digits = currencyDigits(price.currency);
  return new Intl.NumberFormat(LOCALE, { style: "currency", currency: price.currency }).format(price.amount / 10 ** digits);
}

/** A typed amount (`19.99` or `19,99`) in `currency` → its price in minor units; `null` when
 * empty; otherwise the reason it can't be one. */
export function parsePrice(text: string, currency: string): { amount: number; currency: string } | null | string {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const match = /^(\d+)(?:[.,](\d*))?$/.exec(trimmed);
  if (!match) return "Enter an amount such as 19.99.";
  const digits = currencyDigits(currency);
  const decimals = match[2] ?? "";
  if (decimals.length > digits) return digits === 0 ? `${currency} has no decimals.` : `Use at most ${digits} decimals.`;
  const amount = Number(match[1]) * 10 ** digits + Number(decimals.padEnd(digits, "0") || "0");
  if (amount > MAX_PRICE_AMOUNT) return `The most is ${formatPrice({ amount: MAX_PRICE_AMOUNT, currency })}.`;
  return { amount, currency };
}

export function formatBytes(bytes: number): string {
  const units = ["bytes", "KB", "MB", "GB"];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${size} bytes` : `${size.toFixed(1)} ${units[unit]}`;
}
