import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../api/client';
import {
  accepted,
  analysis,
  author,
  draft,
  editor,
  extracted,
  notAccepted,
  submission,
  version,
} from '../test/fixtures';
import { KATEX_ERROR, MACROS } from '../test/katex';
import { fakeApi, renderWithApp } from '../test/render';
import { SubmissionDetailPage } from './SubmissionDetail';

describe('paper workspace', () => {
  it('refreshes a delayed conjecture target on focus after the review has settled', async () => {
    const initial: typeof accepted = {
      ...accepted,
      kind: 'conjecture' as const,
      claims: [{ ...accepted.claims[2]!, role: 'main' as const }],
      lean_statements: [],
    };
    let current = initial;
    const getSubmission = vi.fn(async () => current);
    renderWithApp(
      <SubmissionDetailPage id={initial.id} />,
      fakeApi(
        {
          getSubmission,
          getAnalysis: vi.fn(async () => analysis),
          previewDecision: vi.fn(async () => initial.decision!),
        },
        author,
      ),
    );
    expect(await screen.findByText('The Lean statement is being prepared.')).toBeInTheDocument();
    current = {
      ...initial,
      revision: initial.revision + 1,
      lean_statements: [
        {
          claim: 'C3',
          version: 1,
          claims_revision: initial.claims_revision,
          digest: 'a'.repeat(64),
          lean: 'theorem wishpool_target : True := by sorry',
          toolchain: 'Lean test',
          reading: 'A delayed translation.',
          response: { state: 'awaiting_author' as const },
          created_at: '2026-10-10T00:00:00Z',
        },
      ],
    };
    fireEvent.focus(window);
    expect(await screen.findByText('A delayed translation.')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Yes, this is my conjecture' })).toBeEnabled();
  });

  it('lets the author confirm the extracted statements', async () => {
    const confirmClaims = vi.fn(async () => submission);
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => draft),
        previewDecision: vi.fn(async () => draft.decision!),
        confirmClaims,
      },
      author,
    );
    renderWithApp(<SubmissionDetailPage id="sub-draft" />, api);
    const user = userEvent.setup();

    const form = await screen.findByRole('form', { name: 'Confirm statements' });
    expect(screen.getByText(/has no number; using "Conjecture"/)).toBeInTheDocument();
    // The statement is rendered with KaTeX.
    expect(form.querySelector('.katex')).not.toBeNull();

    await user.click(within(form).getByRole('checkbox', { name: 'C1 uses C2' }));
    await user.selectOptions(within(form).getByLabelText('Kind of C2'), 'proposition');
    await user.selectOptions(within(form).getByLabelText('Role of C3'), 'main');
    await user.click(within(form).getByRole('checkbox', { name: 'Exclude C4' }));
    await user.click(screen.getByRole('button', { name: 'Confirm statements and start review' }));

    expect(confirmClaims).toHaveBeenCalledWith('sub-draft', [
      { id: 'C1', kind: 'theorem', role: 'main', depends_on: ['C2'] },
      { id: 'C2', kind: 'proposition', role: 'supporting', depends_on: [] },
      { id: 'C3', kind: 'conjecture', role: 'main', depends_on: [] },
      { id: 'C4', kind: 'claim', role: 'supporting', excluded: true },
    ]);
    await waitFor(() =>
      expect(screen.queryByRole('form', { name: 'Confirm statements' })).toBeNull(),
    );
  });

  it('renders the confirm table with the current version’s macros and full select labels', async () => {
    const withMacros = {
      ...draft,
      // Only the current (last) version's macros apply.
      versions: [version(1), version(2, { macros: MACROS })],
      extracted: [
        { ...extracted[0]!, statement: 'The map $\\rep$ satisfies $\\code{ab}$.' },
        ...extracted.slice(1),
      ],
    };
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => withMacros),
        previewDecision: vi.fn(async () => draft.decision!),
      },
      author,
    );
    renderWithApp(<SubmissionDetailPage id="sub-draft" />, api);
    const form = await screen.findByRole('form', { name: 'Confirm statements' });
    expect(form.querySelector('.mop')).toHaveTextContent('rep');
    expect(form.querySelector(KATEX_ERROR)).toBeNull();

    // Kind and Role sit in their own fitted columns and show their full option text.
    const kind = within(form).getByLabelText('Kind of C2');
    const role = within(form).getByLabelText('Role of C2');
    expect(kind).toHaveDisplayValue('Lemma');
    expect(role).toHaveDisplayValue('Supporting');
    expect(within(form).getByLabelText('Role of C1')).toHaveDisplayValue('Main result');
    expect(
      within(kind as HTMLSelectElement)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['Theorem', 'Proposition', 'Lemma', 'Corollary', 'Claim', 'Conjecture', 'Question']);
    expect(kind.closest('td')).toHaveClass('confirm-kind');
    expect(kind.closest('td')).toHaveAttribute('data-label', 'Kind');
    expect(role.closest('td')).toHaveClass('confirm-role');
    expect(role.closest('td')).toHaveAttribute('data-label', 'Role');
    expect(within(form).getByRole('columnheader', { name: 'Kind' })).toHaveClass('confirm-kind');
  });

  it('shows the server’s reason when the confirmation is refused', async () => {
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => draft),
        previewDecision: vi.fn(async () => draft.decision!),
        confirmClaims: vi.fn(async () => {
          throw new ApiError({
            status: 422,
            code: 'invalid',
            detail: 'claim dependencies contain a cycle',
          });
        }),
      },
      author,
    );
    renderWithApp(<SubmissionDetailPage id="sub-draft" />, api);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Confirm statements and start review' }),
    );
    expect(await screen.findByText('claim dependencies contain a cycle')).toBeInTheDocument();
  });

  it('gives a paper that was not accepted its reasons and a new-version form', async () => {
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => notAccepted),
        previewDecision: vi.fn(async () => notAccepted.decision!),
        getAnalysis: vi.fn(async () => ({ ...analysis, submission: 'sub-na' })),
      },
      author,
    );
    renderWithApp(<SubmissionDetailPage id="sub-na" />, api);

    const section = await screen.findByRole('region', { name: 'Paper standing' });
    expect(
      within(section).getByText('Statement C1 already follows from 1901.00001.'),
    ).toBeInTheDocument();
    expect(
      within(section).getByText('No checked main result carries new mathematical content.'),
    ).toBeInTheDocument();
    await userEvent.click(
      within(section).getByRole('button', { name: 'Upload a revised version' }),
    );
    expect(screen.getByRole('form', { name: 'Upload a new version' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Editor tools' })).toBeNull();
  });

  it('lets the author of an accepted paper choose visibility and answer a formalization proposal', async () => {
    const setVisibility = vi.fn(async () => ({
      ...accepted,
      analysis_visibility: 'public' as const,
    }));
    const respondFormalization = vi.fn(async () => accepted);
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => accepted),
        previewDecision: vi.fn(async () => accepted.decision!),
        getAnalysis: vi.fn(async () => analysis),
        setVisibility,
        respondFormalization,
      },
      author,
    );
    renderWithApp(<SubmissionDetailPage id="sub-acc" />, api);
    const user = userEvent.setup();

    expect(await screen.findByRole('link', { name: 'Public page WP-2026-0001' })).toHaveAttribute(
      'href',
      '/papers/WP-2026-0001',
    );
    await user.click(screen.getByRole('radio', { name: /^Public/ }));
    expect(setVisibility).toHaveBeenCalledWith('sub-acc', 'public');

    await user.click(screen.getByRole('button', { name: 'Decline' }));
    expect(await screen.findByText('Give a reason for declining.')).toBeInTheDocument();
    await user.type(screen.getByLabelText('Reason (needed when declining)'), 'Not before v2.');
    await user.click(screen.getByRole('button', { name: 'Decline' }));
    expect(respondFormalization).toHaveBeenCalledWith('sub-acc', 'C1', {
      approve: false,
      reason: 'Not before v2.',
    });
  });

  it('shows editors their tools and the 409 detail when deciding too early', async () => {
    const api = fakeApi(
      {
        getSubmission: vi.fn(async () => submission),
        previewDecision: vi.fn(async () => submission.decision!),
        getAnalysis: vi.fn(async () => ({ ...analysis, submission: 'sub-1' })),
        applyDecision: vi.fn(async () => {
          throw new ApiError({
            status: 409,
            code: 'conflict',
            detail: 'awaiting S2: S2 needs a human report',
          });
        }),
      },
      editor,
    );
    renderWithApp(<SubmissionDetailPage id="sub-1" />, api);
    expect(await screen.findByRole('heading', { name: 'Editor tools' })).toBeInTheDocument();
    expect(screen.getByText('Pending — awaiting S2 Literature')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Apply decision' }));
    expect(await screen.findByText('awaiting S2: S2 needs a human report')).toBeInTheDocument();
  });
});
