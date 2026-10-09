import { describe, expect, it } from 'vitest';
import type { TaskKind } from '../api/types';
import {
  OUTPUT_FOR_KIND,
  TASK_KINDS,
  TASK_KIND_VERIFICATION,
  buildContribution,
  emptyContributionFields,
  isReviewedKind,
  type ContributionFields,
} from './tasks';

function fields(patch: Partial<ContributionFields>): ContributionFields {
  return { ...emptyContributionFields(), tool: 'Claude Code', model: 'claude-opus-5-5', ...patch };
}

describe('task kinds', () => {
  it('states what makes each kind count', () => {
    expect(TASK_KINDS).toHaveLength(4);
    expect(TASK_KIND_VERIFICATION.judge_escape).toMatch(/different model family/);
    expect(TASK_KIND_VERIFICATION.formalize).toMatch(/standard axioms/);
    expect(isReviewedKind('literature_check')).toBe(true);
    expect(isReviewedKind('probe')).toBe(true);
    expect(isReviewedKind('judge_escape')).toBe(false);
    expect(isReviewedKind('formalize')).toBe(false);
  });
});

describe('buildContribution', () => {
  it('builds a judgement with witnesses one per line', () => {
    const built = buildContribution(
      'judge_escape',
      fields({
        shape: 'content',
        witnesses: ' the pairing $k$ \n\n a new bound ',
        rationale: ' new ',
        inputTokens: '1,200',
        outputTokens: '300',
      }),
    );
    expect(built).toEqual({
      ok: true,
      value: {
        agent: { tool: 'Claude Code', model: 'claude-opus-5-5' },
        output: {
          output: 'judgement',
          shape: 'content',
          witnesses: ['the pairing $k$', 'a new bound'],
          rationale: 'new',
        },
        tokens: { input: 1200, output: 300 },
      },
    });
  });

  it('drops witnesses from a bind-only judgement and requires them for content', () => {
    const bind = buildContribution(
      'judge_escape',
      fields({ shape: 'bind_only', witnesses: 'leftover', rationale: 'r' }),
    );
    expect(bind.ok && bind.value.output).toEqual({
      output: 'judgement',
      shape: 'bind_only',
      witnesses: [],
      rationale: 'r',
    });
    expect(buildContribution('judge_escape', fields({ rationale: 'r' }))).toEqual({
      ok: false,
      error: 'A content judgement names at least one escape witness.',
    });
  });

  it('builds a literature check', () => {
    const prior = [
      {
        claim: 'C1',
        source: { kind: 'arxiv' as const, locator: '1901.00001' },
        relation: 'implies' as const,
        note: ' Thm 3 ',
      },
    ];
    const built = buildContribution(
      'literature_check',
      fields({ prior, searched: 'OpenAlex\nzbMATH Open', summary: 'Implied by Thm 3.' }),
    );
    expect(built.ok && built.value).toEqual({
      agent: { tool: 'Claude Code', model: 'claude-opus-5-5' },
      output: {
        output: 'literature',
        prior: [{ ...prior[0], note: 'Thm 3' }],
        searched: ['OpenAlex', 'zbMATH Open'],
        summary: 'Implied by Thm 3.',
      },
    });
  });

  it('builds a probe note and a pull request', () => {
    expect(buildContribution('probe', fields({ note: ' Checked $n \\le 10^6$. ' }))).toMatchObject({
      ok: true,
      value: { output: { output: 'probe_note', note: 'Checked $n \\le 10^6$.' } },
    });
    expect(
      buildContribution('formalize', fields({ url: 'https://github.com/ada/sums-lean/pull/4' })),
    ).toMatchObject({
      ok: true,
      value: { output: { output: 'pull_request', url: 'https://github.com/ada/sums-lean/pull/4' } },
    });
    expect(buildContribution('formalize', fields({ url: 'javascript:alert(1)' })).ok).toBe(false);
  });

  it('uses the output kind the task kind expects', () => {
    const filled = fields({
      rationale: 'r',
      witnesses: 'w',
      searched: 's',
      summary: 's',
      note: 'n',
      url: 'https://example.org/pr/1',
    });
    for (const kind of TASK_KINDS as TaskKind[]) {
      const built = buildContribution(kind, filled);
      expect(built.ok && built.value.output.output).toBe(OUTPUT_FOR_KIND[kind]);
    }
  });

  it('requires the agent and rejects malformed token counts', () => {
    expect(buildContribution('probe', { ...fields({ note: 'n' }), tool: '' }).ok).toBe(false);
    expect(buildContribution('probe', fields({ note: 'n', inputTokens: '12k' }))).toEqual({
      ok: false,
      error: 'Token counts are whole numbers.',
    });
  });
});
