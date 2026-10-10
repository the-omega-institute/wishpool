import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type { AuditedClaim, Correctness, RefereeRound } from '../api/types';
import { submission } from '../test/fixtures';
import { queuedRound, refereeAudit, refereeRound } from '../test/referee';
import { StatementSummary } from './StatementSummary';

function round(correctness: Correctness, extra: Partial<AuditedClaim> = {}): RefereeRound {
  return {
    ...refereeRound,
    audit: {
      attempts: 1,
      state: {
        state: 'done',
        at: '',
        result: {
          ...refereeAudit,
          claims: [{ ...refereeAudit.claims[0]!, correctness, ...extra }, refereeAudit.claims[1]!],
        },
      },
    },
  };
}

describe('statement summary', () => {
  it('lists every confirmed statement, groups main results first, and expands the full text', async () => {
    render(
      <StatementSummary
        submission={{ ...submission, claims: [...submission.claims].reverse() }}
        round={refereeRound}
      />,
    );
    const section = screen.getByRole('region', { name: 'Statements' });
    expect(within(section).queryByRole('table')).toBeNull();
    expect(within(section).getByRole('heading', { name: 'Main result', level: 3 })).toBeVisible();
    expect(
      within(section).getByRole('heading', { name: 'Supporting · 2', level: 3 }),
    ).toBeVisible();
    const [main, supporting] = within(section).getAllByRole('list');
    expect(within(main!).getAllByRole('listitem')).toHaveLength(1);
    expect(within(supporting!).getAllByRole('listitem')).toHaveLength(2);
    const toggles = within(section).getAllByRole('button');
    expect(toggles).toHaveLength(submission.claims.length);
    expect(toggles[0]).toHaveTextContent('Theorem C1');
    expect(toggles[1]).toHaveTextContent('Conjecture C3');
    expect(toggles[2]).toHaveTextContent('Lemma C2');
    const toggle = toggles[0]!;
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(toggle.querySelector('.katex')).not.toBeNull();
    const body = document.getElementById(toggle.getAttribute('aria-controls')!)!;
    expect(body).not.toBeVisible();
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(body).toBeVisible();
    expect(body).toHaveTextContent('For every');
    expect(body.querySelector('.katex')).not.toBeNull();
    expect(within(body).getByText(refereeAudit.claims[0]!.comment)).toBeVisible();
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(body).not.toBeVisible();
  });

  it('uses the statement title without its LaTeX key while retaining the full statement', async () => {
    render(
      <StatementSummary
        submission={{
          ...submission,
          claims: [
            { ...submission.claims[0]!, label: 'Theorem (The summation formula) [thm:sum]' },
          ],
        }}
      />,
    );
    const toggle = screen.getByRole('button', { name: /^Theorem C1.*The summation formula$/ });
    expect(toggle).not.toHaveTextContent('thm:sum');
    expect(toggle).not.toHaveTextContent('For every');
    await userEvent.click(toggle);
    const body = document.getElementById(toggle.getAttribute('aria-controls')!)!;
    expect(body).toBeVisible();
    expect(body).toHaveTextContent('For every');
    expect(body.querySelector('.katex')).not.toBeNull();
  });

  it('counts multiple main results in the group heading', () => {
    render(
      <StatementSummary
        submission={{
          ...submission,
          claims: submission.claims.map((claim) =>
            claim.id === 'C2' ? { ...claim, role: 'main' } : claim,
          ),
        }}
      />,
    );
    expect(screen.getByRole('heading', { name: 'Main results · 2', level: 3 })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Supporting · 1', level: 3 })).toBeVisible();
  });

  it.each([
    ['correct', 'Correct'],
    ['gap', 'Gap'],
    ['error', 'Error'],
    ['not_checked', 'Not checked'],
  ] as const)(
    'shows audited %s and keeps comments in the intended place',
    async (correctness, label) => {
      render(<StatementSummary submission={submission} round={round(correctness)} />);
      const toggle = screen.getByRole('button', { name: /^Theorem C1/ });
      expect(within(toggle).getByText(label)).toBeVisible();
      if (correctness === 'correct') expect(within(toggle).getByText('New')).toBeVisible();
      else expect(within(toggle).queryByText('New')).toBeNull();
      const item = toggle.closest('li')!;
      const comment = refereeAudit.claims[0]!.comment;
      const visibleComments = within(item)
        .getAllByText(comment)
        .filter((node) => !node.closest('[hidden]'));
      if (correctness === 'gap' || correctness === 'error') {
        expect(visibleComments).toHaveLength(1);
        expect(visibleComments[0]).toBeVisible();
        expect(visibleComments[0]!.previousElementSibling).toBe(toggle);
      } else expect(visibleComments).toHaveLength(0);
      await userEvent.click(toggle);
      expect(within(item).getByText(comment)).toBeVisible();
    },
  );

  it.each([
    [{ shape: 'bind_only', witnesses: [] }, 'Follows from known results'],
    [{ known: 'A named earlier theorem' }, 'Already known'],
  ] satisfies [Partial<AuditedClaim>, string][])(
    'shows main-result novelty from the audit',
    (reading, novelty) => {
      render(<StatementSummary submission={submission} round={round('correct', reading)} />);
      expect(
        within(screen.getByRole('button', { name: /^Theorem C1/ })).getByText(novelty),
      ).toBeVisible();
      expect(
        within(screen.getByRole('button', { name: /^Lemma C2/ })).queryByText(
          /^(New|Follows from known results|Already known)$/,
        ),
      ).toBeNull();
    },
  );

  it('marks statements missing from a completed audit as not checked', () => {
    render(<StatementSummary submission={submission} round={refereeRound} />);
    expect(
      within(screen.getByRole('button', { name: /^Conjecture C3/ })).getByText('Not checked'),
    ).toBeVisible();
  });

  it('shows proved Lean tags and reveals the theorem name on expansion', async () => {
    render(
      <StatementSummary
        submission={submission}
        round={{
          ...refereeRound,
          formal: {
            attempts: 0,
            state: {
              state: 'done',
              at: '',
              result: {
                attempts: [
                  { claim: 'C1', outcome: 'compiled', theorem: 'Wishpool.C1.main' },
                  { claim: 'C2', outcome: 'failed' },
                ],
              },
            },
          },
        }}
      />,
    );
    const toggle = screen.getByRole('button', { name: /^Theorem C1/ });
    expect(within(toggle).getByText('Lean ✓')).toBeVisible();
    expect(screen.getByText('Wishpool.C1.main')).not.toBeVisible();
    expect(
      within(screen.getByRole('button', { name: /^Lemma C2/ })).queryByText('Lean ✓'),
    ).toBeNull();
    await userEvent.click(toggle);
    expect(screen.getByText('Wishpool.C1.main')).toBeVisible();
    expect(screen.getByText(/Proved in Lean 4 as/)).toBeVisible();
  });

  it('shows a Lean tag for an existing verified formalization without a probe', () => {
    render(
      <StatementSummary
        submission={{
          ...submission,
          formalization: {
            ...submission.formalization,
            items: [
              {
                claim: 'C1',
                reason: 'Verified proof',
                updated_at: '',
                state: {
                  state: 'verified',
                  artifact: {
                    repository: 'https://github.com/ada/sums-lean',
                    commit: 'proof-commit',
                    declarations: ['Sums.main'],
                  },
                  axioms: [],
                },
              },
            ],
          },
        }}
      />,
    );
    expect(
      within(screen.getByRole('button', { name: /^Theorem C1/ })).getByText('Lean ✓'),
    ).toBeVisible();
  });

  it.each([undefined, queuedRound])(
    'omits verdict and novelty tags until an audit exists',
    (review) => {
      render(<StatementSummary submission={submission} round={review} />);
      const section = screen.getByRole('region', { name: 'Statements' });
      expect(within(section).getAllByRole('button')).toHaveLength(submission.claims.length);
      expect(within(section).queryByText(/^(Correct|Gap|Error|Not checked)$/)).toBeNull();
      expect(
        within(section).queryByText(/^(New|Follows from known results|Already known)$/),
      ).toBeNull();
      expect(within(section).queryByText('Awaiting review.')).toBeNull();
    },
  );
});
