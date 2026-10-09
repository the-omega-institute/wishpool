import type { DonationGrant, GrantStatus } from '../api/types';
import type { Parsed } from './parsed';

/** The server's bound on a monthly cap, in tokens. */
export const MAX_MONTHLY_CAP = 50_000_000;
export const DEFAULT_MONTHLY_CAP = 200_000;

export const GRANT_STATUS_LABELS: { readonly [K in GrantStatus]: string } = {
  active: 'Active',
  paused: 'Paused',
  revoked: 'Revoked',
};

/** `YYYY-MM` of `now` in UTC, the period a grant's `used` counter belongs to. */
export function currentPeriod(now: Date): string {
  const month = String(now.getUTCMonth() + 1).padStart(2, '0');
  return `${now.getUTCFullYear()}-${month}`;
}

export interface DonationMeter {
  /** Tokens used in the current month (0 when the grant's counter is from an earlier month). */
  used: number;
  cap: number;
  remaining: number;
  /** Share of the cap used, clamped to 0–100. */
  percent: number;
  /** The grant's `period` is not the current month: its counter has not rolled over yet. */
  rolledOver: boolean;
}

/**
 * Usage of a donated grant this month. The server resets `used` lazily, on
 * the first metered call of a new month, so a grant whose `period` is not
 * the current month has used nothing yet this month.
 */
export function donationMeter(
  grant: Pick<DonationGrant, 'monthly_cap' | 'used' | 'period'>,
  now: Date = new Date(),
): DonationMeter {
  const rolledOver = grant.period !== currentPeriod(now);
  const cap = Number.isFinite(grant.monthly_cap) ? Math.max(0, grant.monthly_cap) : 0;
  const rawUsed = Number.isFinite(grant.used) ? Math.max(0, grant.used) : 0;
  const used = rolledOver ? 0 : rawUsed;
  const percent = cap === 0 ? (used > 0 ? 100 : 0) : Math.min(100, (used / cap) * 100);
  return { used, cap, remaining: Math.max(0, cap - used), percent, rolledOver };
}

/** A monthly cap typed into a form: a whole number of tokens within the server's bounds. */
export function parseCap(text: string): Parsed<number> {
  const cleaned = text.replace(/[\s,_]/g, '');
  if (!/^\d+$/.test(cleaned)) return { ok: false, error: 'The cap is a whole number of tokens.' };
  const cap = Number(cleaned);
  if (cap < 1 || cap > MAX_MONTHLY_CAP) {
    return {
      ok: false,
      error: `The cap must be between 1 and ${MAX_MONTHLY_CAP.toLocaleString('en-GB')} tokens.`,
    };
  }
  return { ok: true, value: cap };
}

/** Model names the venue accepts: 3–100 characters of letters, digits and `- . _ /`. */
export function isValidModelName(model: string): boolean {
  return /^[A-Za-z0-9._/-]{3,100}$/.test(model);
}
