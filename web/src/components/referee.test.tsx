import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, type ApiClient } from '../api/client';
import type { Person, RefereeRound, Submission } from '../api/types';
import { useReferee } from '../api/useReferee';
import { SubmissionDetailPage } from '../pages/SubmissionDetail';
import {
  accepted,
  analysis,
  author,
  contributor,
  editor,
  notAccepted,
  submission,
  version,
} from '../test/fixtures';
import { KATEX_ERROR, MACROS } from '../test/katex';
import {
  feedbackLetter,
  queuedRound,
  refereeAdvice,
  refereeFile,
  refereeReport,
  refereeRound,
} from '../test/referee';
import { fakeApi, renderWithApp } from '../test/render';
import { RefereeWorkspace } from './referee';

const reviewer: Person = { ...editor, id: 'nyx|reviewer', roles: ['reviewer'] };

function workspace(overrides: Partial<ApiClient> = {}, person = editor, paper = submission) {
  const api = fakeApi(
    {
      getSubmission: vi.fn(async () => ({ ...paper, versions: [version(1, { macros: MACROS })] })),
      getAnalysis: vi.fn(async () => analysis),
      previewDecision: vi.fn(async () => paper.decision!),
      referee: vi.fn(async () => refereeFile()),
      ...overrides,
    },
    person,
  );
  return { ...renderWithApp(<SubmissionDetailPage id={paper.id} />, api), api };
}

async function staffPanel() {
  const panel = (await screen.findByRole('heading', { name: 'Review', level: 2 })).closest(
    'section',
  )!;
  await within(panel).findAllByRole('list', { name: 'Review progress' });
  return panel;
}

afterEach(() => {
  vi.useRealTimers();
});

describe('referee workspace', () => {
  it('shows one status line, the queued position, pending steps and the engine on hover', async () => {
    workspace({ referee: vi.fn(async () => refereeFile({ rounds: [queuedRound] })) });
    const panel = await staffPanel();
    expect(within(panel).getByRole('status')).toHaveTextContent(
      'Referee report: waiting in queue (#7).',
    );
    expect(within(panel).getByText(/In queue \(#7\)/)).toBeInTheDocument();
    expect(within(panel).getAllByText('Pending')).toHaveLength(4);
    expect(panel.querySelector('li[title="NyxID Oracle · ChatGPT Pro"]')).not.toBeNull();
    expect(within(panel).getByRole('button', { name: 'Start a new review' })).toBeDisabled();
    expect(within(panel).getByRole('list', { name: 'Review progress' })).toHaveTextContent(
      'Referee',
    );
  });

  it('shows a running step without a queue position', async () => {
    const round: RefereeRound = {
      ...queuedRound,
      referee: {
        ...queuedRound.referee,
        state: { state: 'running', since: refereeRound.started_at },
      },
    };
    workspace({ referee: vi.fn(async () => refereeFile({ rounds: [round] })) });
    const panel = await staffPanel();
    expect(within(panel).getByText(/Running · since/)).toBeInTheDocument();
    expect(within(panel).queryByText(/Waiting in queue/)).toBeNull();
  });

  it('renders the report as advisory, severity groups, readings, limits and the full answer with paper macros', async () => {
    workspace();
    const panel = await staffPanel();
    expect(within(panel).getAllByText('Done')).toHaveLength(5);
    await userEvent.click(within(panel).getByText('Referee report · minor revision'));
    expect(within(panel).getByText('Referee recommends minor revision')).toBeInTheDocument();
    expect(
      within(panel).getByText(/publication decision are recorded separately/),
    ).toBeInTheDocument();
    expect(within(panel).getByRole('heading', { name: 'Major concerns' })).toBeInTheDocument();
    expect(within(panel).getByRole('heading', { name: 'Minor concerns' })).toBeInTheDocument();
    expect(
      within(panel).getByRole('heading', { name: 'Limits of this review' }),
    ).toBeInTheDocument();
    const readings = within(panel).getByRole('table', {
      name: 'The referee’s per-statement readings',
    });
    expect(within(readings).getByText('Content')).toBeInTheDocument();
    expect(within(readings).getByText('Bind-only')).toBeInTheDocument();
    expect(within(readings).getByText('A standard order argument.')).toBeInTheDocument();
    expect(within(readings).getByRole('link', { name: 'Theorem 1.1' })).toHaveAttribute(
      'href',
      '#ws-C1',
    );
    expect(document.getElementById('ws-C1')).toHaveAttribute('id', 'ws-C1');
    await userEvent.click(within(panel).getByText('Full referee answer'));
    expect(panel.querySelector('.mop')).toHaveTextContent('rep');
    expect(panel.querySelector(KATEX_ERROR)).toBeNull();
    expect(
      screen.getByRole('heading', { name: 'Publication decision', level: 2 }),
    ).toBeInTheDocument();
  });

  it('renders an unreadable recommendation without inventing one', async () => {
    const report = { ...refereeReport, recommendation: undefined };
    const round: RefereeRound = {
      ...refereeRound,
      referee: {
        ...refereeRound.referee,
        state: { state: 'done', result: report, at: refereeRound.started_at },
      },
    };
    workspace({ referee: vi.fn(async () => refereeFile({ rounds: [round] })) });
    const panel = await staffPanel();
    await userEvent.click(within(panel).getByText('Referee report · No recommendation'));
    expect(within(panel).getByText('No readable recommendation was returned.')).toBeInTheDocument();
    expect(within(panel).queryByText(/^Referee recommends/)).toBeNull();
  });

  it('shows failed and skipped reasons with expandable failure details', async () => {
    const round: RefereeRound = {
      ...refereeRound,
      referee: {
        attempts: 2,
        state: {
          state: 'failed',
          reason: 'Oracle unavailable',
          detail: 'The request timed out.',
          retryable: true,
          at: refereeRound.started_at,
        },
      },
      advice: { attempts: 0, state: { state: 'skipped', reason: 'No report to advise on' } },
      letter: { attempts: 0, state: { state: 'skipped', reason: 'No draft available' } },
    };
    workspace({ referee: vi.fn(async () => refereeFile({ rounds: [round] })) });
    const panel = await staffPanel();
    expect(within(panel).getByText('Failed: Oracle unavailable')).toBeInTheDocument();
    expect(within(panel).getByText('Skipped: No report to advise on')).toBeInTheDocument();
    await userEvent.click(within(panel).getByText('Failure details'));
    expect(within(panel).getByText('The request timed out.')).toBeInTheDocument();
    expect(within(panel).getByRole('button', { name: 'Start a new review' })).toBeEnabled();
  });

  it('distinguishes checked and proposed improvements and shows the checked evidence', async () => {
    workspace();
    const panel = await staffPanel();
    await userEvent.click(
      within(panel).getByText('Suggestions · 2 improvements, 1 statements that could go to Lean'),
    );
    const table = within(panel).getByRole('table', { name: 'Suggested improvements' });
    expect(within(table).getByText('Checked', { selector: '.badge' })).toHaveClass('badge-good');
    expect(within(table).getByText('Proposed', { selector: '.badge' })).toHaveClass('badge-warn');
    expect(within(table).getByText('For n = 1 both sides equal 1.')).toBeInTheDocument();
    expect(within(table).getByText('Whole paper')).toBeInTheDocument();
    expect(within(panel).getByText('Ready', { selector: '.badge' })).toBeInTheDocument();
    expect(within(panel).getByText('A wrapper lemma.')).toBeInTheDocument();
    expect(panel.querySelector('.code-block pre')).toHaveTextContent('theorem square_injective');
    expect(within(panel).queryByRole('button', { name: 'Propose formalization' })).toBeNull();
  });

  it('shows the Lean probe as private, with outcome, axioms, note and source', async () => {
    workspace();
    const panel = await staffPanel();
    await userEvent.click(within(panel).getByText('Lean check · 1 of 1 proved'));
    const probe = within(panel)
      .getByRole('heading', { name: 'Lean formalization probe' })
      .closest('div')!;
    expect(within(probe).getByText('Compiled in Lean', { selector: '.badge' })).toHaveClass(
      'badge-good',
    );
    expect(within(probe).getByText(/^Private ·/)).toHaveTextContent('Mathlib v4.33.0');
    expect(
      within(probe).getByText('Axioms: propext, Classical.choice, Quot.sound'),
    ).toBeInTheDocument();
    expect(within(probe).getByText('Wishpool.C2.main')).toBeInTheDocument();
    expect(within(probe).getByText('Lean source')).toBeInTheDocument();
    await userEvent.click(within(probe).getByText('Lean source'));
    expect(probe.querySelector('.code-block pre')).toHaveTextContent('import Mathlib');
    expect(within(panel).getByText('Lean check')).toBeInTheDocument();
  });

  it('posts the edited subject, body and note and displays the returned sent letter', async () => {
    const sendFeedback = vi.fn(async () => feedbackLetter);
    workspace({ sendFeedback });
    const panel = await staffPanel();
    const form = within(panel).getByRole('form', { name: 'Send feedback to the author' });
    expect(within(form).getByLabelText('Assessment')).toHaveDisplayValue('Minor revision');
    const user = userEvent.setup();
    const fields = {
      Subject: 'Revised editorial feedback',
      Body: 'A **checked** result: $\\rep$.',
      Note: 'A note using $\\code{x}$.',
    };
    for (const [label, value] of Object.entries(fields)) {
      await user.clear(within(form).getByLabelText(label));
      await user.paste(value);
    }
    await user.click(within(form).getByText('Preview', { selector: 'summary' }));
    const preview = within(form).getByRole('region', { name: 'Feedback preview' });
    expect(within(preview).getByText('checked', { selector: 'strong' })).toBeInTheDocument();
    expect(preview.querySelector('.mop')).toHaveTextContent('rep');
    expect(preview.querySelector(KATEX_ERROR)).toBeNull();
    await user.click(within(form).getByRole('button', { name: 'Send to author' }));
    expect(sendFeedback).toHaveBeenCalledExactlyOnceWith(submission.id, {
      subject: fields.Subject,
      body: fields.Body,
      note: fields.Note,
      assessment: 'minor_revision',
    });
    expect(
      await within(panel).findByRole('heading', { name: 'Sent editorial feedback', level: 3 }),
    ).toBeInTheDocument();
    expect(within(panel).getByText(`Sent by ${editor.id}`)).toBeInTheDocument();
    expect(within(panel).getByText('Edited', { selector: '.badge' })).toBeInTheDocument();
    expect(within(panel).queryByRole('form', { name: 'Send feedback to the author' })).toBeNull();
    expect(within(panel).getAllByRole('status')[0]).toHaveTextContent(
      'Feedback sent to the authors on',
    );
  });

  it('warns on an empty body and preserves the draft when sending fails', async () => {
    const sendFeedback = vi.fn(async () => {
      throw new ApiError({ status: 409, code: 'conflict', detail: 'The current draft changed.' });
    });
    workspace({ sendFeedback });
    const panel = await staffPanel();
    const user = userEvent.setup();
    await user.clear(within(panel).getByLabelText('Body'));
    await user.click(within(panel).getByRole('button', { name: 'Send to author' }));
    expect(within(panel).getByRole('alert')).toHaveTextContent('The letter body is empty.');
    expect(sendFeedback).not.toHaveBeenCalled();
    await user.type(within(panel).getByLabelText('Body'), 'Specific feedback.');
    await user.click(within(panel).getByRole('button', { name: 'Send to author' }));
    expect(await within(panel).findByRole('alert')).toHaveTextContent('The current draft changed.');
    expect(within(panel).getByLabelText('Body')).toHaveValue('Specific feedback.');
    expect(within(panel).queryByText('Feedback sent to the author.')).toBeNull();
  });

  it.each(['accept', 'minor_revision', 'major_revision', 'reject', undefined] as const)(
    'defaults the letter assessment to the recommendation %s',
    async (recommendation) => {
      workspace({
        referee: vi.fn(async () =>
          refereeFile({
            rounds: [
              {
                ...refereeRound,
                referee: {
                  ...refereeRound.referee,
                  state: {
                    state: 'done',
                    result: { ...refereeReport, recommendation },
                    at: refereeRound.started_at,
                  },
                },
              },
            ],
          }),
        ),
      });
      const select = within(await staffPanel()).getByLabelText('Assessment');
      expect(select).toHaveValue(recommendation ?? '');
      expect(
        within(select)
          .getAllByRole('option')
          .map((option) => option.textContent),
      ).toEqual(['Accept', 'Minor revision', 'Major revision', 'Reject', 'No assessment']);
    },
  );

  it.each(['reject', ''] as const)(
    'sends the editor’s chosen assessment %s',
    async (assessment) => {
      const sendFeedback = vi.fn(async () => ({
        ...feedbackLetter,
        assessment: assessment || undefined,
      }));
      workspace({ sendFeedback });
      const form = within(await staffPanel()).getByRole('form', {
        name: 'Send feedback to the author',
      });
      await userEvent.selectOptions(within(form).getByLabelText('Assessment'), assessment);
      await userEvent.click(within(form).getByRole('button', { name: 'Send to author' }));
      expect(sendFeedback).toHaveBeenCalledExactlyOnceWith(submission.id, {
        subject:
          refereeRound.letter.state.state === 'done'
            ? refereeRound.letter.state.result.subject
            : '',
        body:
          refereeRound.letter.state.state === 'done' ? refereeRound.letter.state.result.body : '',
        note:
          refereeRound.letter.state.state === 'done' ? refereeRound.letter.state.result.note : '',
        ...(assessment ? { assessment } : {}),
      });
    },
  );

  it.each([editor, author])(
    'shows the sent assessment beside the subject for $id',
    async (person) => {
      workspace(
        {
          referee: vi.fn(async () =>
            refereeFile({ letters: [{ ...feedbackLetter, assessment: 'minor_revision' }] }),
          ),
        },
        person,
      );
      const heading = await screen.findByRole('heading', {
        name: 'Sent editorial feedback Minor revision',
      });
      expect(
        within(heading).getByText('Minor revision', { selector: '.badge' }),
      ).toBeInTheDocument();
    },
  );

  it('shows assessments in the staff’s earlier-letter history', async () => {
    workspace({
      referee: vi.fn(async () =>
        refereeFile({ letters: [{ ...feedbackLetter, round: undefined, assessment: 'reject' }] }),
      ),
    });
    const panel = await staffPanel();
    await userEvent.click(within(panel).getByText('Earlier reviews'));
    const badge = within(panel).getByText('Reject', { selector: '.badge' });
    expect(badge.closest('summary')).toHaveTextContent(feedbackLetter.subject);
  });

  it('gives authors sent letters newest first and folds the full referee report', async () => {
    workspace(
      {
        referee: vi.fn(async () =>
          refereeFile({
            letters: [
              { ...feedbackLetter, subject: 'Earlier feedback', sent_at: '2026-10-07T12:00:00Z' },
              feedbackLetter,
            ],
          }),
        ),
      },
      author,
    );
    const heading = await screen.findByRole('heading', { name: feedbackLetter.subject });
    const panel = heading.closest('section')!;
    expect(panel).toHaveAccessibleName('Letter from the editors');
    const letters = heading.closest('ul')!;
    expect(
      within(letters)
        .getAllByRole('heading', { level: 3 })
        .map((h) => h.textContent),
    ).toEqual(['Sent editorial feedback', 'Earlier feedback']);
    expect(panel.querySelector('.mop')).toHaveTextContent('rep');
    expect(panel.querySelector(KATEX_ERROR)).toBeNull();
    expect(within(panel).getAllByText('Note', { selector: 'summary' })).toHaveLength(2);
    expect(screen.queryByRole('heading', { name: 'Review' })).toBeNull();
    const fullReport = within(panel).getByText('Full referee report', { selector: 'summary' });
    expect(fullReport.closest('details')).not.toHaveAttribute('open');
    expect(within(panel).getByText(/Referee recommends/)).not.toBeVisible();
    await userEvent.click(fullReport);
    expect(within(panel).getByText(/Referee recommends/)).toBeVisible();
    expect(screen.queryByRole('heading', { name: 'Contributor advice' })).toBeNull();
    expect(screen.queryByRole('form', { name: 'Send feedback to the author' })).toBeNull();
  });

  it('shows nothing for an author with no sent letters', async () => {
    const referee = vi.fn(async () => refereeFile());
    workspace({ referee }, author);
    await waitFor(() => {
      expect(referee).toHaveBeenCalledOnce();
      expect(screen.queryByRole('region', { name: 'Letter from the editors' })).toBeNull();
    });
    expect(screen.queryByText('Full referee report', { selector: 'summary' })).toBeNull();
    expect(screen.queryByText('No feedback letters have been sent.')).toBeNull();
  });

  it('previews the first two paragraphs of a long letter and reveals the rest and note on expansion', async () => {
    const paragraphs = [
      'Dear author,',
      'We checked the induction step.',
      'Please clarify the boundary case.',
      'Sincerely, the editors.',
    ];
    workspace(
      {
        referee: vi.fn(async () =>
          refereeFile({
            letters: [
              {
                ...feedbackLetter,
                body: paragraphs.join('\n\n'),
                note: 'A detailed editorial note.',
              },
            ],
          }),
        ),
      },
      author,
    );
    const heading = await screen.findByRole('heading', { name: feedbackLetter.subject });
    const panel = heading.closest('section')!;
    for (const paragraph of paragraphs.slice(0, 2))
      expect(within(panel).getByText(paragraph)).toBeVisible();
    for (const paragraph of paragraphs.slice(2))
      expect(within(panel).queryByText(paragraph)).toBeNull();
    expect(within(panel).queryByText('Note', { selector: 'summary' })).toBeNull();
    expect(within(panel).queryByText('A detailed editorial note.')).toBeNull();
    await userEvent.click(within(panel).getByRole('button', { name: 'Read the full letter' }));
    for (const paragraph of paragraphs) expect(within(panel).getByText(paragraph)).toBeVisible();
    expect(within(panel).queryByRole('button', { name: 'Read the full letter' })).toBeNull();
    const note = within(panel).getByText('Note', { selector: 'summary' });
    expect(note).toBeVisible();
    expect(within(panel).getByText('A detailed editorial note.')).not.toBeVisible();
    await userEvent.click(note);
    expect(within(panel).getByText('A detailed editorial note.')).toBeVisible();
  });

  it('shows a letter of three paragraphs in full without an expansion button', async () => {
    const paragraphs = ['Dear author,', 'The result is correct.', 'Sincerely, the editors.'];
    workspace(
      {
        referee: vi.fn(async () =>
          refereeFile({
            letters: [{ ...feedbackLetter, body: paragraphs.join('\n\n'), note: '' }],
          }),
        ),
      },
      author,
    );
    const heading = await screen.findByRole('heading', { name: feedbackLetter.subject });
    const panel = heading.closest('section')!;
    for (const paragraph of paragraphs) expect(within(panel).getByText(paragraph)).toBeVisible();
    expect(within(panel).queryByRole('button', { name: 'Read the full letter' })).toBeNull();
    expect(within(panel).queryByText('Note', { selector: 'summary' })).toBeNull();
  });

  it('gives reviewers and admins the staff view, with mutations reserved for editors and admins', async () => {
    const view = workspace({}, reviewer);
    let panel = await staffPanel();
    await userEvent.click(within(panel).getByText('Referee report · minor revision'));
    expect(within(panel).getByRole('heading', { name: 'Referee report' })).toBeInTheDocument();
    expect(within(panel).queryByRole('button', { name: /referee round/ })).toBeNull();
    expect(within(panel).queryByRole('button', { name: 'Send to author' })).toBeNull();
    view.unmount();
    workspace({}, { ...editor, roles: ['admin'] });
    panel = await staffPanel();
    expect(within(panel).getByRole('button', { name: 'Send to author' })).toBeInTheDocument();
  });

  it('does not request referee data for a non-author without a staff role', async () => {
    const referee = vi.fn(async () => refereeFile());
    workspace({ referee }, contributor);
    await screen.findByRole('heading', { name: submission.title });
    expect(referee).not.toHaveBeenCalled();
  });

  it('starts an absent round and restarts only a settled paper in review', async () => {
    const restartReferee = vi.fn(async () => refereeFile({ rounds: [queuedRound] }));
    const view = workspace({
      referee: vi.fn(async () => refereeFile({ rounds: [] })),
      restartReferee,
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Start review' }));
    expect(restartReferee).toHaveBeenCalledExactlyOnceWith(submission.id);
    expect(await screen.findByRole('button', { name: 'Start a new review' })).toBeDisabled();
    view.unmount();
    workspace({}, editor, accepted);
    expect(
      within(await staffPanel()).getByRole('button', { name: 'Start a new review' }),
    ).toBeDisabled();
  });

  it('restarts a settled round and shows a conflict without losing the current report', async () => {
    const restartReferee = vi.fn(async () => {
      throw new ApiError({ status: 409, code: 'conflict', detail: 'A round is already active.' });
    });
    workspace({ restartReferee });
    const panel = await staffPanel();
    await userEvent.click(within(panel).getByRole('button', { name: 'Start a new review' }));
    expect(restartReferee).toHaveBeenCalledExactlyOnceWith(submission.id);
    expect(await within(panel).findByRole('alert')).toHaveTextContent('A round is already active.');
    await userEvent.click(within(panel).getByText('Referee report · minor revision'));
    expect(within(panel).getByRole('heading', { name: 'Referee report' })).toBeInTheDocument();
  });

  it('retains earlier rounds in expandable history, without an old send form', async () => {
    workspace({
      referee: vi.fn(async () =>
        refereeFile({ rounds: [refereeRound, { ...queuedRound, number: 2, version: 2 }] }),
      ),
    });
    const panel = (await screen.findByRole('heading', { name: 'Review' })).closest('section')!;
    await within(panel).findAllByRole('list', { name: 'Review progress' });
    await userEvent.click(within(panel).getByText('Earlier reviews'));
    await userEvent.click(within(panel).getByText('Referee report · minor revision'));
    expect(within(panel).getByRole('heading', { name: 'Referee report' })).toBeInTheDocument();
    expect(within(panel).queryByRole('form', { name: 'Send feedback to the author' })).toBeNull();
  });

  it('proposes an eligible accepted statement with the candidate plan through the existing endpoint', async () => {
    const proposed: Submission = {
      ...accepted,
      formalization: {
        ...accepted.formalization,
        items: [
          ...accepted.formalization.items,
          {
            claim: 'C2',
            reason: refereeAdvice.formalization[0]!.plan,
            updated_at: feedbackLetter.sent_at,
            state: { state: 'proposed' },
          },
        ],
      },
    };
    const proposeFormalization = vi.fn(async () => proposed);
    workspace({ proposeFormalization }, editor, accepted);
    const panel = await staffPanel();
    await userEvent.click(
      within(panel).getByText('Suggestions · 2 improvements, 1 statements that could go to Lean'),
    );
    await userEvent.click(within(panel).getByRole('button', { name: 'Propose formalization' }));
    expect(proposeFormalization).toHaveBeenCalledExactlyOnceWith(
      accepted.id,
      'C2',
      refereeAdvice.formalization[0]!.plan,
    );
    await waitFor(() =>
      expect(within(panel).queryByRole('button', { name: 'Propose formalization' })).toBeNull(),
    );
  });

  it('hides formalization actions for reviewers, existing proposals, conjectures and unknown statements', async () => {
    const view = workspace({}, reviewer, accepted);
    let panel = await staffPanel();
    await userEvent.click(
      within(panel).getByText('Suggestions · 2 improvements, 1 statements that could go to Lean'),
    );
    expect(within(panel).queryByRole('button', { name: 'Propose formalization' })).toBeNull();
    view.unmount();
    const candidate = refereeAdvice.formalization[0]!;
    const advice = {
      ...refereeAdvice,
      formalization: ['C1', 'C3', 'missing'].map((claim) => ({ ...candidate, claim })),
    };
    const round: RefereeRound = {
      ...refereeRound,
      advice: {
        ...refereeRound.advice,
        state: { state: 'done', result: advice, at: refereeRound.started_at },
      },
    };
    workspace({ referee: vi.fn(async () => refereeFile({ rounds: [round] })) }, editor, accepted);
    panel = await staffPanel();
    await userEvent.click(
      within(panel).getByText('Suggestions · 2 improvements, 3 statements that could go to Lean'),
    );
    expect(within(panel).queryByRole('button', { name: 'Propose formalization' })).toBeNull();
  });

  it('puts the editable letter after progress and keeps evidence and preview closed', async () => {
    workspace();
    const panel = await staffPanel();
    const progress = within(panel).getByRole('list', { name: 'Review progress' });
    const letter = progress.nextElementSibling!;
    expect(within(letter as HTMLElement).getByRole('form')).toHaveAccessibleName(
      'Send feedback to the author',
    );
    for (const summary of [
      'Preview',
      'Referee report · minor revision',
      'Suggestions · 2 improvements, 1 statements that could go to Lean',
      'Lean check · 1 of 1 proved',
    ]) {
      expect(
        within(panel).getByText(summary, { selector: 'summary' }).closest('details'),
      ).not.toHaveAttribute('open');
    }
    expect(within(panel).getByLabelText('Body')).toBeVisible();
    expect(within(panel).getByRole('button', { name: 'Send to author' })).toBeVisible();
    expect(within(panel).getByRole('heading', { name: 'Referee report' })).not.toBeVisible();
    expect(within(panel).getByRole('region', { name: 'Feedback preview' })).not.toBeVisible();
  });

  it('omits the note preview when the note is empty', async () => {
    workspace();
    const panel = await staffPanel();
    await userEvent.clear(within(panel).getByLabelText('Note'));
    await userEvent.click(within(panel).getByText('Preview', { selector: 'summary' }));
    const preview = within(panel).getByRole('region', { name: 'Feedback preview' });
    expect(within(preview).getByRole('heading', { name: 'Body preview' })).toBeInTheDocument();
    expect(within(preview).queryByRole('heading', { name: 'Note preview' })).toBeNull();
  });
});

describe('referee polling', () => {
  function PollingWorkspace({ paper, isStaff }: { paper: Submission; isStaff: boolean }) {
    const resource = useReferee(paper, isStaff);
    return (
      <RefereeWorkspace
        submission={paper}
        isStaff={isStaff}
        isEditor={isStaff}
        onChange={() => {}}
        resource={resource}
      />
    );
  }

  function polling(referee: ApiClient['referee'], isStaff = true, paper = submission) {
    const api = fakeApi({ referee });
    return renderWithApp(<PollingWorkspace paper={paper} isStaff={isStaff} />, api);
  }

  it('polls every 30 seconds and stops once all steps settle', async () => {
    vi.useFakeTimers();
    const referee = vi
      .fn<ApiClient['referee']>()
      .mockResolvedValueOnce(refereeFile({ rounds: [queuedRound] }))
      .mockResolvedValue(refereeFile());
    polling(referee);
    await act(async () => {});
    expect(referee).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(29_999);
    });
    expect(referee).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(referee).toHaveBeenCalledTimes(2);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(90_000);
    });
    expect(referee).toHaveBeenCalledTimes(2);
  });

  it('continues polling while advice is pending even if the referee report is done', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () =>
      refereeFile({ rounds: [{ ...refereeRound, advice: queuedRound.advice }] }),
    );
    polling(referee);
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(referee).toHaveBeenCalledTimes(3);
  });

  it('clears polling and aborts the request on unmount', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () => refereeFile({ rounds: [queuedRound] }));
    const view = polling(referee);
    await act(async () => {});
    const signal = (referee.mock.calls[0] as unknown as Parameters<ApiClient['referee']>)[1]!
      .signal!;
    view.unmount();
    expect(signal.aborted).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(90_000);
    });
    expect(referee).toHaveBeenCalledTimes(1);
  });

  it('retries polling after a temporary fetch failure and stops when the next response settles', async () => {
    vi.useFakeTimers();
    const referee = vi
      .fn<ApiClient['referee']>()
      .mockResolvedValueOnce(refereeFile({ rounds: [queuedRound] }))
      .mockRejectedValueOnce(new Error('Temporary connection failure'))
      .mockResolvedValue(refereeFile());
    polling(referee);
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(screen.getByRole('alert')).toHaveTextContent('Temporary connection failure');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(referee).toHaveBeenCalledTimes(3);
    expect(screen.queryByRole('alert')).toBeNull();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(referee).toHaveBeenCalledTimes(3);
  });

  it('preserves edited letter fields when a polling response arrives', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () =>
      refereeFile({ rounds: [{ ...refereeRound, advice: queuedRound.advice }] }),
    );
    polling(referee);
    await act(async () => {});
    fireEvent.change(screen.getByLabelText('Subject'), { target: { value: 'Editor’s changes' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(referee).toHaveBeenCalledTimes(2);
    expect(screen.getByLabelText('Subject')).toHaveValue('Editor’s changes');
  });

  it('does not poll settled failures', async () => {
    vi.useFakeTimers();
    const failed: RefereeRound = {
      ...refereeRound,
      referee: {
        attempts: 1,
        state: {
          state: 'failed',
          reason: 'Unavailable',
          retryable: true,
          at: refereeRound.started_at,
        },
      },
    };
    const referee = vi.fn(async () => refereeFile({ rounds: [failed] }));
    polling(referee);
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(referee).toHaveBeenCalledTimes(1);
  });

  it('uses the latest round for polling and restart even when an older round is active', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () =>
      refereeFile({ rounds: [{ ...refereeRound, number: 2 }, queuedRound] }),
    );
    polling(referee);
    await act(async () => {});
    expect(screen.getByRole('status')).toHaveTextContent('The feedback letter is ready.');
    expect(screen.getByRole('button', { name: 'Start a new review' })).toBeEnabled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(referee).toHaveBeenCalledTimes(1);
  });

  it('polls and disables restart when the latest round is active despite settled history', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () =>
      refereeFile({ rounds: [refereeRound, { ...queuedRound, number: 2 }] }),
    );
    polling(referee);
    await act(async () => {});
    expect(screen.getByRole('status')).toHaveTextContent('Referee report: waiting in queue (#7).');
    expect(screen.getByRole('button', { name: 'Start a new review' })).toBeDisabled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(referee).toHaveBeenCalledTimes(2);
  });

  it.each([submission, accepted, notAccepted])(
    'polls with no letter section, then refreshes delivered letters on focus while $status.state',
    async (paper) => {
      vi.useFakeTimers();
      const referee = vi
        .fn<ApiClient['referee']>()
        .mockResolvedValueOnce(refereeFile({ rounds: [queuedRound] }))
        .mockResolvedValueOnce(refereeFile({ letters: [feedbackLetter] }))
        .mockResolvedValue(
          refereeFile({ letters: [{ ...feedbackLetter, subject: 'New feedback on focus' }] }),
        );
      const view = polling(referee, false, paper);
      await act(async () => {});
      expect(view.container).toBeEmptyDOMElement();
      expect(screen.queryByRole('region', { name: 'Letter from the editors' })).toBeNull();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(29_999);
      });
      expect(referee).toHaveBeenCalledTimes(1);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1);
      });
      expect(referee).toHaveBeenCalledTimes(2);
      expect(screen.getByRole('region', { name: 'Letter from the editors' })).toBeInTheDocument();
      expect(screen.getByRole('heading', { name: 'Sent editorial feedback' })).toBeInTheDocument();
      await act(async () => {
        fireEvent.focus(window);
      });
      expect(referee).toHaveBeenCalledTimes(3);
      expect(screen.getByRole('heading', { name: 'New feedback on focus' })).toBeInTheDocument();
      expect(screen.queryByRole('heading', { name: 'Review' })).toBeNull();
      expect(screen.getByText('Full referee report', { selector: 'summary' })).toBeInTheDocument();
      expect(screen.queryByRole('form', { name: 'Send feedback to the author' })).toBeNull();
    },
  );

  it('keeps refreshing an author view after initial and subsequent transient errors', async () => {
    vi.useFakeTimers();
    const referee = vi
      .fn<ApiClient['referee']>()
      .mockRejectedValueOnce(new Error('Temporary connection failure'))
      .mockResolvedValueOnce(refereeFile())
      .mockRejectedValueOnce(new Error('Another temporary failure'))
      .mockResolvedValue(refereeFile({ letters: [feedbackLetter] }));
    polling(referee, false);
    await act(async () => {});
    expect(screen.getByRole('alert')).toHaveTextContent('Temporary connection failure');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByRole('region', { name: 'Letter from the editors' })).toBeNull();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(screen.getByRole('alert')).toHaveTextContent('Another temporary failure');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(referee).toHaveBeenCalledTimes(4);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('region', { name: 'Letter from the editors' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Sent editorial feedback' })).toBeInTheDocument();
  });

  it('cleans up author polling, focus refresh and the request on unmount', async () => {
    vi.useFakeTimers();
    const referee = vi.fn<ApiClient['referee']>().mockResolvedValue(refereeFile());
    const view = polling(referee, false);
    await act(async () => {});
    const signal = referee.mock.calls[0]![1]!.signal!;
    view.unmount();
    expect(signal.aborted).toBe(true);
    await act(async () => {
      fireEvent.focus(window);
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(referee).toHaveBeenCalledTimes(1);
  });

  it('does not poll or refresh on focus for an author with a draft paper', async () => {
    vi.useFakeTimers();
    const referee = vi.fn(async () => refereeFile({ rounds: [queuedRound] }));
    polling(referee, false, { ...submission, status: { state: 'draft' } });
    await act(async () => {});
    await act(async () => {
      fireEvent.focus(window);
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(referee).toHaveBeenCalledTimes(1);
  });
});
