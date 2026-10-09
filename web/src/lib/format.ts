const DATE = new Intl.DateTimeFormat('en-GB', {
  day: 'numeric',
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
});
const DATE_TIME = new Intl.DateTimeFormat('en-GB', {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  timeZone: 'UTC',
  hour12: false,
});

function parse(iso: string): Date | null {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** `6 October 2026` (UTC). Unparseable input is returned unchanged. */
export function formatDate(iso: string): string {
  const date = parse(iso);
  return date === null ? iso : DATE.format(date);
}

/** `6 Oct 2026, 14:05 UTC`. */
export function formatDateTime(iso: string): string {
  const date = parse(iso);
  return date === null ? iso : `${DATE_TIME.format(date)} UTC`;
}

/** The year of an RFC 3339 timestamp, for citations. */
export function yearOf(iso: string): string {
  const date = parse(iso);
  return date === null ? '' : String(date.getUTCFullYear());
}

const COUNT = new Intl.NumberFormat('en-GB');

/** `1,234,567`. */
export function formatCount(n: number): string {
  return COUNT.format(n);
}
