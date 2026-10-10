import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { accepted, author, contributor } from '../../test/fixtures';
import { fakeApi, renderWithApp } from '../../test/render';
import type { Submission } from '../../api/types';
import { LeanStatementCard } from './AuthorPanels';

const conjecture: Submission = {
  ...accepted,
  kind: 'conjecture',
  claims: [{ ...accepted.claims[2]!, role: 'main' }],
  lean_statements: [
    {
      claim: 'C3',
      version: 1,
      claims_revision: 1,
      digest: 'a'.repeat(64),
      lean: 'theorem wishpool_target : True := by sorry',
      toolchain: 'Lean test',
      reading: 'The exact conjecture, with all quantifiers.',
      response: { state: 'awaiting_author' },
      created_at: '2026-10-10T00:00:00Z',
    },
  ],
};

describe('author Lean statement', () => {
  it('shows the target to readers of the workspace without offering another author’s confirmation', async () => {
    renderWithApp(
      <LeanStatementCard submission={conjecture} onChange={vi.fn()} />,
      fakeApi({}, contributor),
    );
    expect(
      await screen.findByText('Awaiting confirmation from the submitting author.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Yes, this is my conjecture' })).toBeNull();
  });

  it('confirms the exact displayed digest and shows no proof badge', async () => {
    const respondLeanStatement = vi.fn(async () => conjecture);
    const onChange = vi.fn();
    renderWithApp(
      <LeanStatementCard submission={conjecture} onChange={onChange} />,
      fakeApi({ respondLeanStatement }, author),
    );
    expect(screen.getByText(conjecture.lean_statements[0]!.lean)).toBeInTheDocument();
    expect(screen.queryByText('Lean ✓')).not.toBeInTheDocument();
    await userEvent.click(
      await screen.findByRole('button', { name: 'Yes, this is my conjecture' }),
    );
    await waitFor(() =>
      expect(respondLeanStatement).toHaveBeenCalledWith(conjecture.id, {
        digest: 'a'.repeat(64),
        confirm: true,
      }),
    );
    expect(onChange).toHaveBeenCalledWith(conjecture);
  });
  it('requires a correction and sends it for regeneration', async () => {
    const respondLeanStatement = vi.fn(async () => conjecture);
    const user = userEvent.setup();
    renderWithApp(
      <LeanStatementCard submission={conjecture} onChange={vi.fn()} />,
      fakeApi({ respondLeanStatement }, author),
    );
    await user.click(await screen.findByRole('button', { name: 'No, it should say…' }));
    await user.click(screen.getByRole('button', { name: 'Request a revised statement' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Describe what the statement should say.',
    );
    expect(respondLeanStatement).not.toHaveBeenCalled();
    await user.type(
      screen.getByLabelText('What should it say?'),
      'Include every n, including zero.',
    );
    await user.click(screen.getByRole('button', { name: 'Request a revised statement' }));
    expect(respondLeanStatement).toHaveBeenCalledWith(conjecture.id, {
      digest: 'a'.repeat(64),
      confirm: false,
      comment: 'Include every n, including zero.',
    });
  });
});
