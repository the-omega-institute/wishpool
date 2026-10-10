import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { paperSummary, publicPaper, SHA } from '../test/fixtures';
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
    expect(within(statements).getByText('C1')).toBeInTheDocument();
    expect(statements.querySelector('.katex')).not.toBeNull();
    // Only C1 carries a Lean proof.
    expect(within(statements).getAllByText('Lean ✓')).toHaveLength(1);
    expect(
      within(statements).getByRole('link', {
        name: `ada/sums-lean@${SHA.slice(0, 12)}`,
        hidden: true,
      }),
    ).toHaveAttribute('href', `https://github.com/ada/sums-lean/tree/${SHA}`);

    expect(screen.queryByRole('heading', { name: 'Analysis' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Conjecture follow-ups' })).toBeNull();
  });

  it('shows dependencies and new lemmas without review labels or commentary', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        claims: publicPaper.claims!.map((c) => ({ ...c, depends_on: c.id === 'C1' ? ['C3'] : [] })),
        new_content: [{ claim: 'C1', lemmas: ['A new intermediate identity $x = y$.'] }],
      })),
    });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    expect(await screen.findByRole('heading', { name: 'New in this paper' })).toBeInTheDocument();
    expect(screen.getByText('Uses C3.')).toBeInTheDocument();
    expect(screen.getByText(/A new intermediate identity/)).toBeInTheDocument();
    for (const text of [
      'Correct',
      'Gap',
      'Error',
      'Not checked',
      'Analysis',
      'Escape witnesses',
      'Confirmed by an editor',
    ]) {
      expect(screen.queryByText(text)).not.toBeInTheDocument();
    }
  });

  it('marks only verified statements with Lean when the API sends null', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        claims: publicPaper.claims!.map((c) => (c.lean ? c : { ...c, lean: null })),
      })),
    });
    renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    const statements = (await screen.findByRole('heading', { name: 'Statements' })).closest(
      'section',
    ) as HTMLElement;
    expect(within(statements).getAllByText('Lean ✓')).toHaveLength(1);
  });

  it('renders a private record with only its title, authors, kind and record', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        summary: {
          kind: 'note' as const,
          record: 'WP-2026-0002',
          title: 'A short note',
          authors: [{ name: 'Ada Author' }],
        },
      })),
    });
    renderWithApp(<PaperPage record="WP-2026-0002" />, api);
    expect(await screen.findByRole('heading', { name: 'A short note' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Statements' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Abstract' })).not.toBeInTheDocument();
  });

  it('renders statements and the abstract with the paper’s macros', async () => {
    const api = fakeApi({
      getPaper: vi.fn(async () => ({
        ...publicPaper,
        summary: { ...publicPaper.summary, abstract_text: 'We study $\\rep(G)$.' },
        claims: [{ ...publicPaper.claims![0]!, statement: 'The map $\\rep$ is \\emph{onto}.' }],
        macros: MACROS,
      })),
    });
    const { container } = renderWithApp(<PaperPage record="WP-2026-0001" />, api);
    await screen.findByRole('heading', { name: 'Statements' });
    // The abstract, the row preview and the expandable statement body.
    expect(container.querySelectorAll('.mop')).toHaveLength(3);
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
