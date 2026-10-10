import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Task } from '../api/types';
import { contributor, task, taskContext } from '../test/fixtures';
import { KATEX_ERROR, MACROS } from '../test/katex';
import { fakeApi, renderWithApp } from '../test/render';
import { TaskPage } from './Tasks';

function withKind(kind: Task['kind']) {
  return { ...taskContext, task: { ...task, kind } };
}

describe('task page', () => {
  it('shows the statement and its dependencies, and submits a judgement', async () => {
    const submitContribution = vi.fn(async () => ({ contribution: {} as never, nature: null }));
    const getTask = vi.fn(async () => taskContext);
    renderWithApp(<TaskPage id="task-1" />, fakeApi({ getTask, submitContribution }, contributor));
    const user = userEvent.setup();

    expect(await screen.findByRole('heading', { level: 1, name: task.title })).toBeInTheDocument();
    expect(screen.getByText('Statements it uses')).toBeInTheDocument();
    expect(screen.getAllByText('Lemma 2.1').length).toBeGreaterThan(0);

    await user.type(screen.getByLabelText('Agent tool'), 'Claude Code');
    await user.type(screen.getByLabelText('Model'), 'claude-opus-5-5');
    await user.type(screen.getByLabelText('Escape witnesses (one per line)'), 'w1{enter}w2');
    await user.type(screen.getByLabelText('Rationale'), 'Both are new.');
    await user.type(screen.getByLabelText('Input tokens (optional)'), '1000');
    await user.click(screen.getByRole('button', { name: 'Submit' }));

    expect(submitContribution).toHaveBeenCalledWith('task-1', {
      agent: { tool: 'Claude Code', model: 'claude-opus-5-5' },
      output: {
        output: 'judgement',
        shape: 'content',
        witnesses: ['w1', 'w2'],
        rationale: 'Both are new.',
      },
      tokens: { input: 1000, output: 0 },
    });
    await waitFor(() => expect(getTask).toHaveBeenCalledTimes(2));
  });

  it('submits a probe note and a pull request for the other kinds', async () => {
    const submitContribution = vi.fn(async () => ({ contribution: {} as never, nature: null }));
    const user = userEvent.setup();

    const probe = renderWithApp(
      <TaskPage id="task-1" />,
      fakeApi({ getTask: vi.fn(async () => withKind('probe')), submitContribution }, contributor),
    );
    await user.type(await screen.findByLabelText('Agent tool'), 'Codex');
    await user.type(screen.getByLabelText('Model'), 'gpt-6');
    await user.type(screen.getByLabelText('Probe note (Markdown with TeX)'), 'Checked n < 100.');
    await user.click(screen.getByRole('button', { name: 'Submit' }));
    expect(submitContribution).toHaveBeenLastCalledWith('task-1', {
      agent: { tool: 'Codex', model: 'gpt-6' },
      output: { output: 'probe_note', note: 'Checked n < 100.' },
    });
    probe.unmount();

    renderWithApp(
      <TaskPage id="task-1" />,
      fakeApi(
        { getTask: vi.fn(async () => withKind('formalize')), submitContribution },
        contributor,
      ),
    );
    await user.type(await screen.findByLabelText('Agent tool'), 'Codex');
    await user.type(screen.getByLabelText('Model'), 'gpt-6');
    await user.type(
      screen.getByLabelText('Pull request URL'),
      'https://github.com/ada/sums-lean/pull/3',
    );
    await user.click(screen.getByRole('button', { name: 'Submit' }));
    expect(submitContribution).toHaveBeenLastCalledWith('task-1', {
      agent: { tool: 'Codex', model: 'gpt-6' },
      output: { output: 'pull_request', url: 'https://github.com/ada/sums-lean/pull/3' },
    });
  });

  it('builds a literature check with a prior work', async () => {
    const submitContribution = vi.fn(async () => ({ contribution: {} as never, nature: null }));
    renderWithApp(
      <TaskPage id="task-1" />,
      fakeApi(
        { getTask: vi.fn(async () => withKind('literature_check')), submitContribution },
        contributor,
      ),
    );
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Agent tool'), 'Codex');
    await user.type(screen.getByLabelText('Model'), 'gpt-6');
    await user.click(screen.getByRole('button', { name: 'Add prior work' }));
    await user.selectOptions(screen.getByLabelText('Relation'), 'implies');
    await user.selectOptions(screen.getByLabelText('Kind'), 'arxiv');
    await user.type(screen.getByLabelText('Locator'), '1901.00001');
    await user.type(screen.getByLabelText('Searched (one per line)'), 'Reported sources');
    await user.type(screen.getByLabelText('Summary'), 'Implied by Theorem 3.');
    await user.click(screen.getByRole('button', { name: 'Submit' }));
    expect(submitContribution).toHaveBeenCalledWith('task-1', {
      agent: { tool: 'Codex', model: 'gpt-6' },
      output: {
        output: 'literature',
        prior: [
          {
            claim: 'C1',
            source: { kind: 'arxiv', locator: '1901.00001' },
            relation: 'implies',
            note: '',
          },
        ],
        searched: ['Reported sources'],
        summary: 'Implied by Theorem 3.',
      },
    });
  });

  it('renders the statement with the paper’s macros', async () => {
    const ctx = {
      ...taskContext,
      claim: { ...taskContext.claim, statement: 'For all $x$, $\\rep(x) = \\code{x}$.' },
      macros: MACROS,
    };
    const { container } = renderWithApp(
      <TaskPage id="task-1" />,
      fakeApi({ getTask: vi.fn(async () => ctx) }, contributor),
    );
    await screen.findByRole('heading', { level: 1, name: task.title });
    expect([...container.querySelectorAll('.mop')].map((e) => e.textContent)).toContain('rep');
    expect(container.querySelector(KATEX_ERROR)).toBeNull();
  });

  it('offers the lease on an open task', async () => {
    const leaseTask = vi.fn(async () => task);
    renderWithApp(
      <TaskPage id="task-1" />,
      fakeApi(
        {
          getTask: vi.fn(async () => ({
            ...taskContext,
            task: { ...task, status: { state: 'open' as const } },
          })),
          leaseTask,
        },
        contributor,
      ),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Lease task' }));
    expect(leaseTask).toHaveBeenCalledWith('task-1', undefined);
    expect(await screen.findByRole('form', { name: 'Submit result' })).toBeInTheDocument();
  });
});
