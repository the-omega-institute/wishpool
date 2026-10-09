import { describe, expect, it } from 'vitest';
import { compareRatios, escapeRateTrend, formatEscapeRate, formatRatio } from './ratio';

describe('escape-rate display', () => {
  it('shows fractions exactly, without reducing them', () => {
    expect(formatRatio({ numerator: 6, denominator: 12 })).toBe('6/12');
    expect(
      formatEscapeRate({
        before: { numerator: 6, denominator: 12 },
        after: { numerator: 2, denominator: 12 },
      }),
    ).toBe('6/12 → 2/12');
    expect(
      formatEscapeRate({
        before: { numerator: 0, denominator: 7 },
        after: { numerator: 0, denominator: 7 },
      }),
    ).toBe('0/7 → 0/7');
  });

  it('compares ratios exactly by cross-multiplication', () => {
    expect(compareRatios({ numerator: 1, denominator: 3 }, { numerator: 2, denominator: 6 })).toBe(
      0,
    );
    expect(compareRatios({ numerator: 1, denominator: 3 }, { numerator: 1, denominator: 2 })).toBe(
      -1,
    );
    expect(compareRatios({ numerator: 3, denominator: 4 }, { numerator: 2, denominator: 3 })).toBe(
      1,
    );
  });

  it('describes the direction of change', () => {
    const r = (a: number, b: number, c: number, d: number) => ({
      before: { numerator: a, denominator: b },
      after: { numerator: c, denominator: d },
    });
    expect(escapeRateTrend(r(6, 12, 2, 12))).toBe('decreased');
    expect(escapeRateTrend(r(1, 2, 2, 4))).toBe('unchanged');
    expect(escapeRateTrend(r(1, 4, 1, 2))).toBe('increased');
    expect(escapeRateTrend(r(1, 0, 1, 2))).toBe('undefined');
  });
});
