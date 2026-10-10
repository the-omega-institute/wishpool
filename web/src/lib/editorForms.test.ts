import { describe, expect, it } from 'vitest';
import { SHA } from '../test/fixtures';
import {
  buildConjectureState,
  buildJudgement,
  buildLiteratureReport,
  buildVerification,
} from './editorForms';

const prior = [
  {
    claim: 'C1',
    source: { kind: 'doi' as const, locator: '10.1/a' },
    relation: 'related' as const,
    note: '',
  },
  {
    claim: 'C1',
    source: { kind: 'arxiv' as const, locator: '1901.00001' },
    relation: 'implies' as const,
    note: ' Thm 2 ',
  },
];

describe('editor forms', () => {
  it('builds a passing S2 report', () => {
    expect(
      buildLiteratureReport({
        outcome: 'pass',
        knownBy: 0,
        question: '',
        summary: ' Nothing states it. ',
        searched: 'Reported sources\n\nzbMATH Open',
        prior: [prior[0]!],
      }),
    ).toEqual({
      ok: true,
      value: {
        report: {
          outcome: { outcome: 'pass' },
          summary: 'Nothing states it.',
          payload: {
            stage: 'literature',
            prior: [prior[0]],
            searched: ['Reported sources', 'zbMATH Open'],
          },
        },
      },
    });
  });

  it('builds a failing S2 report from the prior work that implies a main result', () => {
    const built = buildLiteratureReport({
      outcome: 'fail',
      knownBy: 1,
      question: '',
      summary: 'Known.',
      searched: 'Reported sources',
      prior,
    });
    expect(built.ok && built.value.report.outcome).toEqual({
      outcome: 'fail',
      reason: {
        reason: 'known_result',
        claim: 'C1',
        prior: { kind: 'arxiv', locator: '1901.00001' },
      },
    });
    // A merely related work cannot be the reason.
    expect(
      buildLiteratureReport({
        outcome: 'fail',
        knownBy: 0,
        question: '',
        summary: 'x',
        searched: 'y',
        prior,
      }).ok,
    ).toBe(false);
  });

  it('builds judgements, verifications and conjecture states', () => {
    expect(buildJudgement('bind_only', 'ignored', ' r ')).toEqual({
      ok: true,
      value: { shape: 'bind_only', witnesses: [], rationale: 'r' },
    });
    expect(buildJudgement('content', '', 'r').ok).toBe(false);

    expect(
      buildVerification({
        repository: 'https://github.com/ada/sums-lean',
        commit: SHA.toUpperCase(),
        declarations: 'Sums.main, Sums.aux',
        axioms: 'propext Quot.sound',
        contribution: '',
      }),
    ).toEqual({
      ok: true,
      value: {
        artifact: {
          repository: 'https://github.com/ada/sums-lean',
          commit: SHA,
          declarations: ['Sums.main', 'Sums.aux'],
        },
        axioms: ['propext', 'Quot.sound'],
      },
    });
    expect(
      buildVerification({
        repository: 'https://github.com/a/b',
        commit: 'abc',
        declarations: 'x',
        axioms: '',
        contribution: '',
      }).ok,
    ).toBe(false);
    expect(
      buildVerification({
        repository: 'https://github.com/a/b',
        commit: SHA,
        declarations: 'x',
        axioms: 'propext sorryAx',
        contribution: '',
      }),
    ).toEqual({ ok: false, error: 'Only standard axioms are accepted; found sorryAx.' });

    expect(
      buildConjectureState({ state: 'taken_up', reason: '', outcome: 'proved', summary: '' }),
    ).toEqual({
      ok: true,
      value: { state: 'taken_up' },
    });
    expect(
      buildConjectureState({
        state: 'settled',
        reason: '',
        outcome: 'partial',
        summary: ' n ≤ 10 ',
      }),
    ).toEqual({
      ok: true,
      value: { state: 'settled', outcome: 'partial', summary: 'n ≤ 10', evidence: [] },
    });
    expect(
      buildConjectureState({ state: 'not_pursued', reason: '', outcome: 'proved', summary: '' }).ok,
    ).toBe(false);
  });
});
