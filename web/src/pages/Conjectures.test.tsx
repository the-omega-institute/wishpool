import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { fakeApi, renderWithApp } from '../test/render';
import { ConjecturesPage } from './Conjectures';

describe('public conjectures', () => {
  it('lists open statements and Lean statement status for anonymous readers', async () => {
    const listConjectures = vi.fn(async () => ({
      items: [
        {
          record: 'WP-2026-0003',
          title: 'An open estimate',
          statement: 'For all $n > 1$ ...',
          lean_statement_status: 'confirmed' as const,
        },
        {
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
      '/papers/WP-2026-0003',
    );
    expect(screen.getByText('Lean statement confirmed by author')).toBeInTheDocument();
    expect(screen.getByText('Lean statement awaiting author')).toBeInTheDocument();
    expect(screen.queryByText('Lean verified')).not.toBeInTheDocument();
    expect(listConjectures).toHaveBeenCalledWith({ before: null, limit: 25 }, expect.anything());
  });
});
