import type { EscapeRate, Ratio } from '../api/types';

/**
 * A ratio exactly as reported: `6/12` stays `6/12`. The denominator is the
 * size of the finite arena and reducing the fraction would hide it.
 */
export function formatRatio(ratio: Ratio): string {
  return `${ratio.numerator}/${ratio.denominator}`;
}

/** `6/12 → 2/12`: the escape rate on the arena before and after the claim. */
export function formatEscapeRate(rate: Pick<EscapeRate, 'before' | 'after'>): string {
  return `${formatRatio(rate.before)} → ${formatRatio(rate.after)}`;
}

/** Compare two ratios exactly (cross-multiplication, no floating point). */
export function compareRatios(a: Ratio, b: Ratio): -1 | 0 | 1 {
  const left = a.numerator * b.denominator;
  const right = b.numerator * a.denominator;
  return left < right ? -1 : left > right ? 1 : 0;
}

/** The direction of the change in escape rate, for a screen-reader-friendly label. */
export function escapeRateTrend(rate: Pick<EscapeRate, 'before' | 'after'>): string {
  if (rate.before.denominator === 0 || rate.after.denominator === 0) return 'undefined';
  switch (compareRatios(rate.after, rate.before)) {
    case -1:
      return 'decreased';
    case 1:
      return 'increased';
    default:
      return 'unchanged';
  }
}
