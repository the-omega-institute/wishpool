import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { policy, submission } from '../test/fixtures';
import { StageTimeline } from './StageTimeline';

function stage(name: RegExp) {
  return screen.getByRole('listitem', { name });
}

describe('StageTimeline', () => {
  it('renders S0–S3 with the current report per stage and its history', () => {
    render(
      <StageTimeline
        submission={submission}
        policy={policy}
        decision={{
          decision: 'pending',
          awaiting: 'literature',
          detail: 'S2 needs a human report',
        }}
      />,
    );

    const timeline = screen.getByRole('list', { name: 'Review stages' });
    const headings = within(timeline).getAllByRole('heading', { level: 3 });
    expect(headings.map((h) => h.textContent)).toEqual([
      'S0 Source',
      'S1 Statements',
      'S2 Literature',
      'S3 Escape analysis',
    ]);

    const s0 = stage(/S0 Source/);
    expect(within(s0).getByText('Passed')).toBeInTheDocument();
    expect(within(s0).getByText('Machine · automath (m-7)')).toBeInTheDocument();

    // S1: the author's latest confirmation is current; the earlier one was replaced.
    const s1 = stage(/S1 Statements/);
    expect(within(s1).getByText('Earlier reports (1)')).toBeInTheDocument();
    expect(within(s1).getByText('Replaced by a later report')).toBeInTheDocument();

    // S2: a machine pass on a judgement stage is a proposal, and the decision waits here.
    const s2 = stage(/S2 Literature/);
    expect(within(s2).getByText('Machine pass — awaiting an editor')).toBeInTheDocument();
    expect(within(s2).getByText('Pass (proposal)')).toBeInTheDocument();
    expect(within(s2).getByText('Awaiting')).toBeInTheDocument();
    expect(within(s2).getByText('S2 needs a human report')).toBeInTheDocument();
    expect(within(s2).getByRole('link', { name: 'doi:10.1000/xyz (1999)' })).toHaveAttribute(
      'href',
      'https://doi.org/10.1000/xyz',
    );
    expect(
      within(s2).getByText('Superseded — filed against an earlier set of statements'),
    ).toBeInTheDocument();

    // S3: only a report against the old statements.
    const s3 = stage(/S3 Escape analysis/);
    expect(within(s3).getByText('Reports superseded by new statements')).toBeInTheDocument();
  });
});
