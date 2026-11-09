// Dates and sizes as people read them. Times are shown in the browser's time zone; expiry dates
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
