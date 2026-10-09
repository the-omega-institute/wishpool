import { describe, expect, it } from 'vitest';
import { donateHref } from '../api/client';
import { currentPeriod, donationMeter, isValidModelName, parseCap } from './donation';

const OCT = new Date('2026-10-08T12:00:00Z');

describe('donation meter', () => {
  it('reports usage in the current month', () => {
    expect(donationMeter({ monthly_cap: 200_000, used: 50_000, period: '2026-10' }, OCT)).toEqual({
      used: 50_000,
      cap: 200_000,
      remaining: 150_000,
      percent: 25,
      rolledOver: false,
    });
  });

  it('clamps the percentage to 0–100', () => {
    expect(donationMeter({ monthly_cap: 100, used: 250, period: '2026-10' }, OCT)).toMatchObject({
      percent: 100,
      remaining: 0,
    });
    expect(donationMeter({ monthly_cap: 100, used: -5, period: '2026-10' }, OCT)).toMatchObject({
      used: 0,
      percent: 0,
    });
    expect(donationMeter({ monthly_cap: 0, used: 0, period: '2026-10' }, OCT).percent).toBe(0);
    expect(donationMeter({ monthly_cap: 0, used: 3, period: '2026-10' }, OCT).percent).toBe(100);
  });

  it('shows nothing used when the counter is from an earlier month', () => {
    expect(donationMeter({ monthly_cap: 1000, used: 900, period: '2026-09' }, OCT)).toEqual({
      used: 0,
      cap: 1000,
      remaining: 1000,
      percent: 0,
      rolledOver: true,
    });
  });

  it('takes the period in UTC', () => {
    expect(currentPeriod(new Date('2026-01-31T23:30:00Z'))).toBe('2026-01');
    expect(currentPeriod(new Date('2026-12-01T00:00:00Z'))).toBe('2026-12');
  });
});

describe('donation form values', () => {
  it('parses caps within the server bounds', () => {
    expect(parseCap('200,000')).toEqual({ ok: true, value: 200_000 });
    expect(parseCap(' 1 000 ')).toEqual({ ok: true, value: 1000 });
    expect(parseCap('0').ok).toBe(false);
    expect(parseCap('50000001').ok).toBe(false);
    expect(parseCap('1.5').ok).toBe(false);
    expect(parseCap('').ok).toBe(false);
  });

  it('accepts model names the venue accepts', () => {
    expect(isValidModelName('claude-opus-5-5')).toBe(true);
    expect(isValidModelName('org/model_v1.2')).toBe(true);
    expect(isValidModelName('ab')).toBe(false);
    expect(isValidModelName('has space')).toBe(false);
  });

  it('builds the consent URL with an encoded return path', () => {
    expect(donateHref(200_000, 'claude-opus-5-5', '/contribute')).toBe(
      '/auth/donate?cap=200000&model=claude-opus-5-5&return_to=%2Fcontribute',
    );
    expect(donateHref(1000, '  ', '/contribute')).toBe(
      '/auth/donate?cap=1000&return_to=%2Fcontribute',
    );
  });
});
