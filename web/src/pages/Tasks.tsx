import { useCallback, useState, type FormEvent } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { usePaged } from '../api/usePaged';
import type {
  ContributionMode,
  ProofShape,
  Task,
  TaskContext,
  TaskKind,
  TaskState,
} from '../api/types';
import { useSession } from '../auth/session';
import { ContributionList } from '../components/contributions';
import { LatexText, MacrosProvider } from '../components/Markdown';
import { Pager } from '../components/Pager';
import { PriorWorkEditor } from '../components/PriorWorkEditor';
import { StatementView } from '../components/claims';
import {
  Async,
  Badge,
  DateText,
  ExternalLink,
  Field,
  InlineError,
  Loading,
  SignInPrompt,
  type BadgeTone,
} from '../components/ui';
import { SHAPE_DESCRIPTIONS, SHAPE_LABELS, shortId } from '../lib/labels';
import { safeHttpUrl } from '../lib/links';
import {
  MODE_LABELS,
  TASK_KINDS,
  TASK_KIND_DESCRIPTIONS,
  TASK_KIND_LABELS,
  TASK_KIND_VERIFICATION,
  TASK_STATES,
  TASK_STATE_LABELS,
  buildContribution,
  emptyContributionFields,
  type ContributionFields,
} from '../lib/tasks';
import { Link } from '../routing/router';

interface Filters {
  kind: TaskKind | '';
  status: TaskState | '';
}

const TASK_TONES: { [K in TaskState]: BadgeTone } = {
  open: 'accent',
  leased: 'warn',
  submitted: 'neutral',
  done: 'good',
  closed: 'muted',
};

export function TasksPage({
  submission,
  initialKind,
}: {
  submission?: string;
  initialKind?: TaskKind;
}) {
  const api = useApi();
  const { session, person } = useSession();
  const [filters, setFilters] = useState<Filters>({ kind: initialKind ?? '', status: 'open' });
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listTasks(
        { kind: filters.kind, status: filters.status, submission, before, limit: 25 },
        { signal },
      ),
    [api, filters, submission],
  );
  const paged = usePaged(person ? fetchPage : null);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Tasks</h1>
          <p className="lede">
            Analysis work on papers whose authors invited volunteer contributors. Lease a task, do
            it with your agent, and submit the result before the lease expires.{' '}
            <Link to={{ kind: 'contribute' }}>How to contribute</Link>
          </p>
          {submission ? (
            <p className="muted small">
              Showing tasks of one paper. <Link to={{ kind: 'tasks' }}>All tasks</Link>
            </p>
          ) : null}
        </div>
      </header>

      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what="see the tasks" />
      ) : null}

      {person ? (
        <>
          <form className="filters" aria-label="Filter tasks" onSubmit={(e) => e.preventDefault()}>
            <div className="field">
              <label htmlFor="task-kind">Kind</label>
              <select
                id="task-kind"
                value={filters.kind}
                onChange={(e) => setFilters({ ...filters, kind: e.target.value as TaskKind | '' })}
              >
                <option value="">Any</option>
                {TASK_KINDS.map((k) => (
                  <option key={k} value={k}>
                    {TASK_KIND_LABELS[k]}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label htmlFor="task-status">Status</label>
              <select
                id="task-status"
                value={filters.status}
                onChange={(e) =>
                  setFilters({ ...filters, status: e.target.value as TaskState | '' })
                }
              >
                <option value="">Any</option>
                {TASK_STATES.map((s) => (
                  <option key={s} value={s}>
                    {TASK_STATE_LABELS[s]}
                  </option>
                ))}
              </select>
            </div>
          </form>

          <Async state={paged.state} onRetry={paged.reload}>
            {(listing) =>
              listing.items.length === 0 ? (
                <p className="muted">No tasks match.</p>
              ) : (
                <ul className="entry-list task-list">
                  {listing.items.map((t) => (
                    <li key={t.id}>
                      <TaskRow task={t} />
                    </li>
                  ))}
                </ul>
              )
            }
          </Async>
          <Pager paged={paged} />
        </>
      ) : null}
    </div>
  );
}

function TaskStatusLine({ task, personId }: { task: Task; personId?: string }) {
  const s = task.status;
  const mine = s.state === 'leased' && s.lease.holder === personId;
  return (
    <>
      {s.state === 'leased' ? (
        <span>
          Leased {mine ? 'by you' : `by ${shortId(s.lease.holder)}`} ({MODE_LABELS[s.lease.mode]})
          until <DateText iso={s.lease.until} withTime />
        </span>
      ) : null}
      {s.state === 'closed' ? <span>Closed: {s.reason}</span> : null}
    </>
  );
}

function TaskRow({ task }: { task: Task }) {
  const { person } = useSession();
  return (
    <article className="task">
      <div className="direction-head">
        <Link to={{ kind: 'task', id: task.id }} className="entry-title">
          {task.title}
        </Link>
        <span className="badge-row">
          <Badge tone="neutral" title={TASK_KIND_VERIFICATION[task.kind]}>
            {TASK_KIND_LABELS[task.kind]}
          </Badge>
          <Badge tone={TASK_TONES[task.status.state]}>{TASK_STATE_LABELS[task.status.state]}</Badge>
        </span>
      </div>
      <p className="entry-meta">
        <span>Statement {task.target.claim}</span>
        <span>
          · {task.contributors.length} contributor{task.contributors.length === 1 ? '' : 's'}
        </span>
        <span>
          · opened <DateText iso={task.created_at} />
        </span>
      </p>
      <p className="entry-meta">
        <TaskStatusLine task={task} personId={person?.id} />
      </p>
    </article>
  );
}

export function TaskPage({ id }: { id: string }) {
  const api = useApi();
  const { session, person } = useSession();
  const load = useCallback((signal: AbortSignal) => api.getTask(id, { signal }), [api, id]);
  const ctx = useAsync(person ? load : null);
  return (
    <div className="page">
      <p className="crumbs">
        <Link to={{ kind: 'tasks' }}>Tasks</Link>
      </p>
      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what="see this task" />
      ) : null}
      <Async state={ctx.state} onRetry={ctx.reload}>
        {(c) => (
          <TaskView
            context={c}
            onTask={(task) => ctx.replace({ ...c, task })}
            onSubmitted={ctx.reload}
          />
        )}
      </Async>
    </div>
  );
}

export function TaskView({
  context,
  onTask,
  onSubmitted,
}: {
  context: TaskContext;
  onTask: (task: Task) => void;
  onSubmitted: () => void;
}) {
  const api = useApi();
  const { person, session } = useSession();
  const task = context.task;
  const s = task.status;
  const mine = s.state === 'leased' && person !== null && s.lease.holder === person.id;
  const donations = session.status === 'signed_in' && session.donationsEnabled;
  const [mode, setMode] = useState<ContributionMode>('own_agent');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const repo = context.repository ? safeHttpUrl(context.repository) : null;

  const run = (call: Promise<Task>) => {
    setBusy(true);
    setError(null);
    call.then(
      (t) => {
        setBusy(false);
        onTask(t);
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };

  return (
    <MacrosProvider macros={context.macros}>
      <article className="task-detail">
        <header className="page-head">
          <div>
            <p className="eyebrow">
              {TASK_KIND_LABELS[task.kind]} ·{' '}
              <Badge tone={TASK_TONES[s.state]}>{TASK_STATE_LABELS[s.state]}</Badge>
            </p>
            <h1>{task.title}</h1>
            <p className="entry-meta">
              <TaskStatusLine task={task} personId={person?.id} />
            </p>
          </div>
        </header>

        <section aria-labelledby="rules-h">
          <h2 id="rules-h">What to do</h2>
          <p>{TASK_KIND_DESCRIPTIONS[task.kind]}</p>
          <p className="small">
            <strong>What counts:</strong> {TASK_KIND_VERIFICATION[task.kind]}
          </p>
          {context.nature ? <p className="small muted">{context.nature}</p> : null}
          {task.kind === 'formalize' ? (
            <p className="small">
              Repository:{' '}
              {repo ? <ExternalLink href={repo}>{context.repository}</ExternalLink> : 'not set yet'}
            </p>
          ) : null}
        </section>

        <section aria-labelledby="paper-h">
          <h2 id="paper-h">Paper</h2>
          <p className="entry-title">{context.paper_title}</p>
          {context.abstract_text ? (
            <LatexText source={context.abstract_text} className="abstract" />
          ) : null}
          {context.pdf_public ? (
            <p className="small muted">The paper is accepted; its PDF is public.</p>
          ) : null}
        </section>

        <section aria-labelledby="statement-h">
          <h2 id="statement-h">Statement</h2>
          <StatementView
            claim={context.claim}
            claims={[context.claim, ...context.dependencies]}
            scope="task"
          />
          {context.dependencies.length > 0 ? (
            <>
              <h3>Statements it uses</h3>
              <ol className="claim-list">
                {context.dependencies.map((d) => (
                  <li key={d.id}>
                    <StatementView
                      claim={d}
                      claims={[context.claim, ...context.dependencies]}
                      scope="task"
                    />
                  </li>
                ))}
              </ol>
            </>
          ) : null}
        </section>

        <section aria-labelledby="work-h">
          <h2 id="work-h">Work on it</h2>
          {s.state === 'open' && person ? (
            <div className="inline-form">
              {donations ? (
                <div className="field">
                  <label htmlFor="lease-mode">Run with</label>
                  <select
                    id="lease-mode"
                    value={mode}
                    onChange={(e) => setMode(e.target.value as ContributionMode)}
                  >
                    <option value="own_agent">My own agent</option>
                    <option value="hosted">The hosted worker on my donated quota</option>
                  </select>
                </div>
              ) : null}
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={() => run(api.leaseTask(task.id, mode === 'own_agent' ? undefined : mode))}
              >
                Lease task
              </button>
            </div>
          ) : null}
          {mine ? (
            <>
              <button
                type="button"
                className="button button-quiet"
                disabled={busy}
                onClick={() => run(api.releaseTask(task.id))}
              >
                Release lease
              </button>
              {s.state === 'leased' && s.lease.mode === 'own_agent' ? (
                <ContributionForm task={task} onSubmitted={onSubmitted} />
              ) : (
                <p className="small">The hosted worker runs this task on your donated quota.</p>
              )}
            </>
          ) : null}
          {s.state === 'leased' && !mine ? (
            <p className="muted small">Another contributor holds the lease.</p>
          ) : null}
          <InlineError error={error} />
        </section>

        <section aria-labelledby="contribs-h">
          <h2 id="contribs-h">Contributions</h2>
          <ContributionList task={task.id} person={person} />
        </section>
      </article>
    </MacrosProvider>
  );
}

/** The result form for a leased task; its fields depend on the task's kind. */
export function ContributionForm({ task, onSubmitted }: { task: Task; onSubmitted: () => void }) {
  const api = useApi();
  const [f, setF] = useState<ContributionFields>(emptyContributionFields);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (key: keyof ContributionFields) => (e: { target: { value: string } }) =>
    setF((prev) => ({ ...prev, [key]: e.target.value }));

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const built = buildContribution(task.kind, f);
    if (!built.ok) {
      setError(built.error);
      return;
    }
    setBusy(true);
    setError(null);
    api.submitContribution(task.id, built.value).then(
      () => {
        setBusy(false);
        onSubmitted();
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };

  return (
    <form className="tool-form" onSubmit={submit} aria-label="Submit result" noValidate>
      <h3>Submit result</h3>
      <div className="form-row">
        <Field label="Agent tool" htmlFor="c-tool" hint="e.g. Claude Code, Codex CLI">
          <input id="c-tool" value={f.tool} onChange={set('tool')} />
        </Field>
        <Field label="Model" htmlFor="c-model">
          <input id="c-model" value={f.model} onChange={set('model')} />
        </Field>
      </div>

      {task.kind === 'judge_escape' ? (
        <>
          <fieldset className="radio-list">
            <legend>Shape</legend>
            {(['content', 'bind_only'] as ProofShape[]).map((shape) => (
              <label key={shape} className="radio">
                <input
                  type="radio"
                  name="c-shape"
                  checked={f.shape === shape}
                  onChange={() => setF((prev) => ({ ...prev, shape }))}
                />
                <span>
                  <strong>{SHAPE_LABELS[shape]}</strong> — {SHAPE_DESCRIPTIONS[shape]}
                </span>
              </label>
            ))}
          </fieldset>
          {f.shape === 'content' ? (
            <Field label="Escape witnesses (one per line)" htmlFor="c-witnesses">
              <textarea id="c-witnesses" rows={3} value={f.witnesses} onChange={set('witnesses')} />
            </Field>
          ) : null}
          <Field label="Rationale" htmlFor="c-rationale">
            <textarea id="c-rationale" rows={4} value={f.rationale} onChange={set('rationale')} />
          </Field>
        </>
      ) : null}

      {task.kind === 'literature_check' ? (
        <>
          <PriorWorkEditor
            idPrefix="c-prior"
            value={f.prior}
            claims={[{ id: task.target.claim, kind: 'theorem', label: task.target.claim }]}
            onChange={(prior) => setF((prev) => ({ ...prev, prior }))}
          />
          <Field label="Searched (one per line)" htmlFor="c-searched">
            <textarea id="c-searched" rows={3} value={f.searched} onChange={set('searched')} />
          </Field>
          <Field label="Summary" htmlFor="c-summary">
            <textarea id="c-summary" rows={3} value={f.summary} onChange={set('summary')} />
          </Field>
        </>
      ) : null}

      {task.kind === 'probe' ? (
        <Field label="Probe note (Markdown with TeX)" htmlFor="c-note">
          <textarea id="c-note" rows={6} value={f.note} onChange={set('note')} />
        </Field>
      ) : null}

      {task.kind === 'formalize' ? (
        <Field label="Pull request URL" htmlFor="c-url">
          <input id="c-url" type="url" value={f.url} onChange={set('url')} />
        </Field>
      ) : null}

      <div className="form-row">
        <Field label="Input tokens (optional)" htmlFor="c-in" hint="Stored as self-reported">
          <input
            id="c-in"
            inputMode="numeric"
            value={f.inputTokens}
            onChange={set('inputTokens')}
          />
        </Field>
        <Field label="Output tokens (optional)" htmlFor="c-out">
          <input
            id="c-out"
            inputMode="numeric"
            value={f.outputTokens}
            onChange={set('outputTokens')}
          />
        </Field>
      </div>
      <button type="submit" className="button" disabled={busy}>
        {busy ? 'Submitting…' : 'Submit'}
      </button>
      <InlineError error={error} />
    </form>
  );
}
