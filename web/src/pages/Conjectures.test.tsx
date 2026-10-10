import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { fakeApi, renderWithApp } from '../test/render';
import { ConjecturesPage } from './Conjectures';

describe('public conjectures', () => {
  it('lists open statements and Lean statement status for anonymous readers', async () => {
    const listConjectures = vi.fn(async () => ({
      items: [
        {
          claim: 'C1',
          source: 'submitted',
          status: 'open' as const,
          attempts: 0,
          solver: null,
          record: 'WP-2026-0003',
          title: 'An open estimate',
          statement: 'For all $n > 1$ ...',
          lean_statement_status: 'confirmed' as const,
        },
        {
          claim: 'C1',
          source: 'submitted',
          status: 'open' as const,
          attempts: 0,
          solver: null,
          record: 'WP-2026-0002',
          title: 'A new question',
          statement: 'Is $x$ finite?',
          lean_statement_status: 'awaiting_author' as const,
        },
      ],
      next_before: null,
    }));
    renderWithApp(<ConjecturesPage />, fakeApi({ listConjectures }));
    expect(await screen.findByRole('link', { name: 'An open estimate' })).toHaveAttribute(
      'href',
      '/conjectures/WP-2026-0003/C1',
    );
    expect(screen.getByText(/Target confirmed by author/)).toBeInTheDocument();
    expect(screen.getByText(/Target awaiting author confirmation/)).toBeInTheDocument();
    expect(screen.queryByText('Lean verified')).not.toBeInTheDocument();
    expect(listConjectures).toHaveBeenCalledWith(
      { before: null, limit: 25, status: 'open' },
      expect.anything(),
    );
  });
});
