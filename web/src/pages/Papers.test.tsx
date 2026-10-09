import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { analysis, paperSummary, publicPaper, SHA } from '../test/fixtures';
import { KATEX_ERROR, MACROS } from '../test/katex';
import { fakeApi, renderWithApp } from '../test/render';
import { PaperPage, PapersPage } from './Papers';

describe('public paper page', () => {
  it('shows the statements with KaTeX and a Lean badge, and hides the analysis when absent', async () => {
    const api = fakeApi({ getPaper: vi.fn(async () => publicPaper) });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);

    expect(
      await screen.findByRole('heading', { level: 1, name: 'On sums of integers' }),
    ).toBeInTheDocument();
    expect(screen.getByText('WP-2026-0001')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'doi:10.48550/arXiv.2609.33421' })).toHaveAttribute(
      'href',
      'https://doi.org/10.48550/arXiv.2609.33421',
    );
    expect(screen.getByRole('link', { name: 'PDF (version 1)' })).toHaveAttribute(
      'href',
      '/api/v1/submissions/sub-acc/files/pdf?version=1',
    );

    const statements = screen
      .getByRole('heading', { name: 'Statements' })
      .closest('section') as HTMLElement;
    expect(within(statements).getByText('Theorem 1.1')).toBeInTheDocument();
    expect(within(statements).getByText('Lean verified')).toBeInTheDocument();
    expect(
      within(statements).getByRole('link', { name: `ada/sums-lean@${SHA.slice(0, 12)}` }),
    ).toHaveAttribute('href', `https://github.com/ada/sums-lean/tree/${SHA}`);
    expect(statements.querySelector('.katex')).not.toBeNull();
    // Only C1 carries a Lean proof.
    expect(within(statements).getAllByText('Lean verified')).toHaveLength(1);

    expect(screen.queryByRole('heading', { name: 'Analysis' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Conjecture follow-ups' })).toBeNull();
    expect(
      screen.getByText('The authors keep the per-statement analysis private.'),
    ).toBeInTheDocument();
  });

  it('shows the analysis and conjecture follow-ups when the author made them public', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        analysis,
        conjectures: [
          {
            claim: 'C3',
            state: { state: 'not_pursued' as const, reason: 'Beyond current methods.' },
            updated_at: '2026-10-06T00:00:00Z',
          },
        ],
      })),
    });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);

    const section = (await screen.findByRole('heading', { name: 'Analysis' })).closest(
      'section',
    ) as HTMLElement;
    expect(within(section).getByText('Confirmed by an editor')).toBeInTheDocument();
    expect(within(section).getByText('Escape witnesses')).toBeInTheDocument();
    expect(within(section).getByText('Related')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Conjecture follow-ups' })).toBeInTheDocument();
    expect(screen.getByText('Beyond current methods.')).toBeInTheDocument();
  });

  it('treats null analysis fields from the server as absent', async () => {
    const bare = {
      claim: 'C3',
      kind: 'conjecture',
      role: 'supporting',
      label: 'Conjecture 4.1',
      prior: [],
      known: false,
      assessment: null,
      judgement: null,
      formalization: null,
    } as unknown as (typeof analysis.claims)[number];
    const api = fakeApi({
      getPaper: vi.fn(async () => ({ ...publicPaper, analysis: { ...analysis, claims: [bare] } })),
    });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    expect(await screen.findByText('Not judged yet.')).toBeInTheDocument();
    expect(screen.getByText('No prior work recorded.')).toBeInTheDocument();
  });

  it('renders statements and the abstract with the paper’s macros', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        summary: { ...publicPaper.summary, abstract_text: 'We study $\\rep(G)$.' },
        claims: [{ ...publicPaper.claims[0]!, statement: 'The map $\\rep$ is \\emph{onto}.' }],
        macros: MACROS,
      })),
    });
    const { container } = renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    await screen.findByRole('heading', { name: 'Statements' });
    expect(container.querySelectorAll('.mop')).toHaveLength(2);
    expect(container.querySelector(KATEX_ERROR)).toBeNull();
  });

  it('renders a missing macros field as no macros', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        macros: null as unknown as Record<string, string>,
      })),
    });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    expect(await screen.findByRole('heading', { name: 'Statements' })).toBeInTheDocument();
  });

  it('lists accepted papers', async () => {
    const api = fakeApi({
      listPapers: vi.fn(async () => ({ items: [paperSummary], next_before: null })),
    });
    renderWithApp(<PapersPage />, api);
    expect(await screen.findByRole('link', { name: 'On sums of integers' })).toHaveAttribute(
      'href',
      '/papers/WP-2026-0001',
    );
    expect(screen.getByText('1 Lean verified')).toBeInTheDocument();
  });
});
