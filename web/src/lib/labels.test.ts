import { describe, expect, it } from 'vitest';
import { paperSummary } from '../test/fixtures';
import { nonStandardAxioms } from './axioms';
import { paperCitation } from './citation';
import { formatDate } from './format';
import {
  conjectureStateText,
  decisionHeadline,
  formatAuthors,
  formatBytes,
  isOpenKind,
  parseMsc,
  rejectReasonText,
} from './labels';
import { stageName } from './stages';

describe('labels', () => {
  it('formats author lists', () => {
    expect(formatAuthors([])).toBe('');
    expect(formatAuthors([{ name: 'A' }])).toBe('A');
    expect(formatAuthors([{ name: 'A' }, { name: 'B' }, { name: 'C' }])).toBe('A, B and C');
  });

  it('parses MSC codes', () => {
    expect(parseMsc(' 11b13, 05D10;11B13  ')).toEqual(['11B13', '05D10']);
    expect(parseMsc('')).toEqual([]);
  });

  it('states every reason a paper is not accepted', () => {
    expect(
      rejectReasonText({
        reason: 'known_result',
        claim: 'C1',
        prior: { kind: 'arxiv', locator: '1' },
      }),
    ).toBe('Known result: statement C1 is stated in, or directly implied by, arXiv:1.');
    expect(rejectReasonText({ reason: 'bind_only' })).toMatch(/^Bind-only: no main result/);
    expect(rejectReasonText({ reason: 'no_main_result' })).toMatch(/^No main result/);
    expect(rejectReasonText({ reason: 'hygiene', detail: 'pdflatex failed' })).toBe(
      'Source: pdflatex failed',
    );
    expect(rejectReasonText({ reason: 'out_of_scope', detail: 'physics' })).toBe(
      'Out of scope: physics',
    );
  });

  it('names decisions, conjecture states and open kinds', () => {
    const name = (s: Parameters<typeof stageName>[0]) => stageName(s);
    expect(
      decisionHeadline({ decision: 'pending', awaiting: 'literature', detail: '' }, name),
    ).toBe('Pending — awaiting S2 Literature');
    expect(decisionHeadline({ decision: 'accept', basis: 'escape_witness' }, name)).toBe(
      'Accept — Escape witness',
    );
    expect(conjectureStateText({ state: 'not_pursued', reason: 'too hard' })).toBe(
      'Not pursued: too hard',
    );
    expect(
      conjectureStateText({ state: 'settled', outcome: 'disproved', summary: '', evidence: [] }),
    ).toBe('Settled — disproved');
    expect(isOpenKind('conjecture')).toBe(true);
    expect(isOpenKind('question')).toBe(true);
    expect(isOpenKind('lemma')).toBe(false);
  });

  it('flags non-standard Lean axioms', () => {
    expect(nonStandardAxioms(['propext', 'Classical.choice', 'Quot.sound'])).toEqual([]);
    expect(nonStandardAxioms(['propext', 'sorryAx'])).toEqual(['sorryAx']);
  });

  it('formats dates, sizes and a citation', () => {
    expect(formatDate('2026-10-01T23:30:00Z')).toBe('1 October 2026');
    expect(formatDate('garbage')).toBe('garbage');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(31_457_280)).toBe('30.0 MB');
    expect(paperCitation(paperSummary, 'https://wishpool.dev')).toBe(
      'Ada Author and Ben Second. On sums of integers. Wishpool record WP-2026-0001, accepted 5 October 2026. doi:10.48550/arXiv.2609.33421 (https://doi.org/10.48550/arXiv.2609.33421). https://wishpool.dev/papers/WP-2026-0001',
    );
  });
});
