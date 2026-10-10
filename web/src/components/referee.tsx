import { useState, type FormEvent } from 'react';
import { useApi } from '../api/context';
import type { RefereeResource } from '../api/useReferee';
import type {
  Advice,
  FeedbackLetter,
  FormalProbe,
  FormalizationCandidate,
  LetterDraft,
  Recommendation,
  RefereeReport,
  RefereeAudit,
  RefereeRound,
  Step,
  Submission,
} from '../api/types';
import {
  EFFORT_LABELS,
  EVIDENCE_LABELS,
  FEASIBILITY_LABELS,
  IMPROVEMENT_KIND_LABELS,
  PROBE_OUTCOME_LABELS,
  RECOMMENDATION_LABELS,
  SEVERITY_LABELS,
  SHAPE_LABELS,
  STEP_STATE_LABELS,
  isOpenKind,
} from '../lib/labels';
import { ClaimRef } from './claims';
import { Markdown } from './Markdown';
import { Async, Badge, DateText, Field, InlineError } from './ui';

type WorkspaceProps = {
  submission: Submission;
  isStaff: boolean;
  isEditor: boolean;
  onChange: (submission: Submission) => void;
};

/** Staff see the full history; authors see delivered letters and referee reports. */
export function RefereeWorkspace(props: WorkspaceProps & { resource: RefereeResource }) {
  const { submission, isStaff, isEditor, resource } = props;
  const api = useApi();
  const { reload, active } = resource;

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const restart = async () => {
    setBusy(true);
    setError(null);
    try {
      resource.replace(await api.restartReferee(submission.id));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  if (!isStaff) {
    // Nothing to show the author until the first letter arrives.
    if (resource.state.status === 'ok' && resource.value?.letters.length === 0) return null;
    return (
      <section className="referee-panel" aria-labelledby="feedback-h">
        <h2 id="feedback-h">Letter from the editors</h2>
        <Async state={resource.state} onRetry={reload}>
          {(file) => {
            const current = [...file.rounds]
              .reverse()
              .find(
                (r) =>
                  r.version === submission.versions.at(-1)?.number &&
                  r.claims_revision === submission.claims_revision,
              );
            return (
              <>
                <SentLetters letters={file.letters} authorView />
                {current?.referee.state.state === 'done' ? (
                  <details className="history">
                    <summary>Full referee report</summary>
                    <ReportView report={current.referee.state.result} submission={submission} />
                  </details>
                ) : null}
              </>
            );
          }}
        </Async>
      </section>
    );
  }

  return (
    <section className="referee-panel" aria-labelledby="referee-h">
      <h2 id="referee-h">Review</h2>
      <Async state={resource.state} onRetry={reload}>
        {(file) => {
          const rounds = [...file.rounds].sort((a, b) => b.number - a.number);
          const current = rounds[0];
          const sent = current
            ? file.letters.filter((letter) => letter.round === current.number)
            : [];
          const earlierLetters = file.letters.filter((letter) => !sent.includes(letter));
          const canRestart = submission.status.state === 'in_review' && !active;
          return (
            <>
              <p className="review-status" role="status">
                {current ? roundStatus(current, sent) : 'No review has started.'}
              </p>
              {current ? (
                <RoundView
                  key={current.number}
                  {...props}
                  round={current}
                  sent={sent}
                  onSent={(letter) =>
                    resource.replace({
                      ...file,
                      letters: [
                        ...file.letters,
                        { ...letter, round: letter.round ?? current.number },
                      ],
                    })
                  }
                />
              ) : null}
              {rounds.length > 1 || earlierLetters.length > 0 ? (
                <details className="history">
                  <summary>Earlier reviews</summary>
                  {rounds.slice(1).map((round) => (
                    <div className="referee-earlier" key={round.number}>
                      <h3>
                        Review of version {round.version} ·{' '}
                        <DateText iso={round.started_at} withTime />
                      </h3>
                      <RoundView
                        {...props}
                        round={round}
                        sent={[]}
                        isEditor={false}
                        onSent={() => {}}
                      />
                    </div>
                  ))}
                  {earlierLetters.length > 0 ? <SentLetters letters={earlierLetters} /> : null}
                </details>
              ) : null}
              {isEditor ? (
                <div className="button-row">
                  <button
                    type="button"
                    className="button button-quiet button-small"
                    disabled={busy || !canRestart}
                    onClick={() => void restart()}
                    title={
                      active
                        ? 'Wait for the current review to finish.'
                        : submission.status.state !== 'in_review'
                          ? 'A review can be started only while the paper is in review.'
                          : undefined
                    }
                  >
                    {current ? 'Start a new review' : 'Start review'}
                  </button>
                </div>
              ) : null}
              <InlineError error={error} />
            </>
          );
        }}
      </Async>
    </section>
  );
}

const STEP_NAMES = {
  referee: 'Referee report',
  audit: 'Audit',
  advice: 'Suggestions',
  formal: 'Lean check',
  letter: 'Letter',
} as const;

/** One sentence: where the latest review stands and what the editor does next. */
function roundStatus(round: RefereeRound, sent: readonly FeedbackLetter[]): string {
  const last = sent[sent.length - 1];
  if (last) return `Feedback sent to the authors on ${formatDay(last.sent_at)}.`;
  const steps = [
    ['referee', round.referee],
    ['audit', round.audit],
    ['advice', round.advice],
    ['letter', round.letter],
    ['formal', round.formal],
  ] as const;
  for (const [key, step] of steps) {
    if (!step) continue;
    const state = step.state;
    if (state.state === 'failed') return `${STEP_NAMES[key]} failed: ${state.reason}`;
    if (state.state === 'running') {
      return state.queue_position !== undefined
        ? `${STEP_NAMES[key]}: waiting in queue (#${state.queue_position}).`
        : `${STEP_NAMES[key]}: in progress.`;
    }
    if (state.state === 'pending') return `${STEP_NAMES[key]}: waiting to start.`;
  }
  if (round.letter?.state.state === 'done') {
    return 'The feedback letter is ready. Read it, edit if needed, and send it to the authors.';
  }
  return 'The review finished without a letter.';
}

function formatDay(iso: string): string {
  return new Date(iso).toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  });
}

function StepProgress({ name, step }: { name: string; step: Step<unknown> }) {
  const state = step.state;
  const engine = [step.engine, step.model].filter(Boolean).join(' · ');
  return (
    <li className={`step-${state.state}`} title={engine || undefined}>
      <strong>{name}</strong>
      <span>
        {state.state === 'running' ? (
          <>
            {state.queue_position !== undefined
              ? `In queue (#${state.queue_position})`
              : STEP_STATE_LABELS.running}
            {' · since '}
            <DateText iso={state.since} withTime />
          </>
        ) : state.state === 'failed' || state.state === 'skipped' ? (
          <>
            {STEP_STATE_LABELS[state.state]}: {state.reason}
          </>
        ) : (
          STEP_STATE_LABELS[state.state]
        )}
      </span>
      {state.state === 'failed' && state.detail ? (
        <details className="history">
          <summary>Failure details</summary>
          <Markdown text={state.detail} />
        </details>
      ) : null}
    </li>
  );
}

function RoundView({
  round,
  sent,
  submission,
  isEditor,
  onChange,
  onSent,
}: WorkspaceProps & {
  round: RefereeRound;
  sent: readonly FeedbackLetter[];
  onSent: (letter: FeedbackLetter) => void;
}) {
  const formal = round.formal?.state.state === 'done' ? round.formal.state.result : null;
  return (
    <div className="referee-round">
      <ol className="referee-progress" aria-label="Review progress">
        <StepProgress name={STEP_NAMES.referee} step={round.referee} />
        {round.audit ? <StepProgress name={STEP_NAMES.audit} step={round.audit} /> : null}
        {round.advice ? <StepProgress name={STEP_NAMES.advice} step={round.advice} /> : null}
        {round.letter ? <StepProgress name={STEP_NAMES.letter} step={round.letter} /> : null}
        {round.formal ? <StepProgress name={STEP_NAMES.formal} step={round.formal} /> : null}
      </ol>
      {sent.length > 0 ? (
        <SentLetters letters={[...sent]} authorView showSender />
      ) : round.letter?.state.state === 'done' ? (
        <div className="tool-form">
          <h3>Letter to the authors</h3>
          {isEditor ? (
            <LetterForm
              key={round.letter.state.at}
              draft={round.letter.state.result}
              recommendation={
                round.referee.state.state === 'done'
                  ? round.referee.state.result.recommendation
                  : undefined
              }
              submissionId={submission.id}
              onSent={onSent}
            />
          ) : (
            <LetterText letter={round.letter.state.result} />
          )}
        </div>
      ) : null}
      {round.referee.state.state === 'done' ? (
        <details className="history">
          <summary>
            Referee report ·{' '}
            {round.referee.state.result.recommendation
              ? RECOMMENDATION_LABELS[round.referee.state.result.recommendation]
              : 'No recommendation'}
          </summary>
          <ReportView report={round.referee.state.result} submission={submission} />
        </details>
      ) : null}
      {round.audit?.state.state === 'done' ? (
        <details className="history">
          <summary>Audit result</summary>
          <AuditView audit={round.audit.state.result} />
        </details>
      ) : null}
      {round.advice?.state.state === 'done' ? (
        <details className="history">
          <summary>
            Suggestions · {round.advice.state.result.improvements.length} improvements,{' '}
            {round.advice.state.result.formalization.length} statements that could go to Lean
          </summary>
          <AdviceView
            advice={round.advice.state.result}
            submission={submission}
            isEditor={isEditor}
            onChange={onChange}
          />
        </details>
      ) : null}
      {formal ? (
        <details className="history">
          <summary>
            Lean check ·{' '}
            {formal.attempts.filter((attempt) => attempt.outcome === 'compiled').length} of{' '}
            {formal.attempts.length} proved
          </summary>
          <ProbeView probe={formal} submission={submission} />
        </details>
      ) : null}
    </div>
  );
}

function StatementRef({ id, submission }: { id?: string; submission: Submission }) {
  if (!id) return <span className="muted">Whole paper</span>;
  const known = submission.claims.some((claim) => claim.id === id);
  return (
    <span>
      {known ? (
        <>
          <code>{id}</code> ·{' '}
        </>
      ) : null}
      <ClaimRef id={id} claims={submission.claims} scope="ws" />
    </span>
  );
}

function MarkdownList({ items }: { items: readonly string[] }) {
  return (
    <ul>
      {items.map((text, i) => (
        <li key={i}>
          <Markdown text={text} />
        </li>
      ))}
    </ul>
  );
}

export function ReportView({
  report,
  submission,
}: {
  report: RefereeReport;
  submission: Submission;
}) {
  return (
    <div className="report">
      <h3>Referee report</h3>
      {report.recommendation ? (
        <Badge tone="accent" title="Referee assessment; the publication criteria decide acceptance">
          Referee recommends {RECOMMENDATION_LABELS[report.recommendation]}
        </Badge>
      ) : (
        <p className="muted">No readable recommendation was returned.</p>
      )}
      <p className="small muted">
        This recommendation is feedback. The source-based check and publication decision are
        recorded separately.
      </p>
      <Markdown text={report.summary} />
      {report.strengths.length > 0 ? (
        <>
          <h4>Strengths</h4>
          <MarkdownList items={report.strengths} />
        </>
      ) : null}
      {(['major', 'minor'] as const).map((severity) => {
        const concerns = report.concerns.filter((concern) => concern.severity === severity);
        if (concerns.length === 0) return null;
        return (
          <div key={severity}>
            <h4>{SEVERITY_LABELS[severity]}</h4>
            <ul>
              {concerns.map((concern, i) => (
                <li key={i}>
                  {concern.claim ? (
                    <StatementRef id={concern.claim} submission={submission} />
                  ) : null}
                  <Markdown text={concern.issue} />
                </li>
              ))}
            </ul>
          </div>
        );
      })}
      {report.claims.length > 0 ? (
        <>
          <h4>Per-statement readings</h4>
          <table className="data-table referee-table">
            <caption className="visually-hidden">The referee’s per-statement readings</caption>
            <thead>
              <tr>
                <th>Statement</th>
                <th>Reading</th>
                <th>Witnesses</th>
                <th>Known source</th>
                <th>Note</th>
              </tr>
            </thead>
            <tbody>
              {report.claims.map((claim, i) => (
                <tr key={i}>
                  <td data-label="Statement">
                    <StatementRef id={claim.claim} submission={submission} />
                  </td>
                  <td data-label="Reading">{SHAPE_LABELS[claim.shape]}</td>
                  <td data-label="Witnesses">
                    {claim.witnesses.length > 0 ? (
                      <MarkdownList items={claim.witnesses} />
                    ) : (
                      'None named'
                    )}
                  </td>
                  <td data-label="Known source">
                    {claim.known ? <Markdown text={claim.known} /> : 'None named'}
                  </td>
                  <td data-label="Note">
                    <Markdown text={claim.note} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      ) : null}
      <h4>Limits of this review</h4>
      {report.limits.length > 0 ? (
        <MarkdownList items={report.limits} />
      ) : (
        <p className="muted">The referee did not specify its limits.</p>
      )}
      <details className="history">
        <summary>Full referee answer</summary>
        <Markdown text={report.text} />
      </details>
    </div>
  );
}

function AuditView({ audit }: { audit: RefereeAudit }) {
  return (
    <div className="report">
      <h3>Audit</h3>
      <Badge>{RECOMMENDATION_LABELS[audit.verdict]}</Badge>
      <p>{audit.summary}</p>
      <p>Agrees with referee: {audit.agrees_with_referee ? 'Yes' : 'No'}</p>
      <ul>
        {audit.claims.map((c) => (
          <li key={c.claim}>
            <strong>
              {c.claim} · {c.correctness}
            </strong>
            <p>{c.comment}</p>
            <p>
              {c.shape ?? 'No proof reading'}
              {c.known ? ` · ${c.known}` : ''}
            </p>
            <MarkdownList items={c.witnesses} />
          </li>
        ))}
      </ul>
      <ul>
        {audit.concerns.map((c, i) => (
          <li key={i}>
            <strong>{c.status}</strong>
            <p>{c.concern}</p>
            <p>{c.note}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}

function AdviceView({
  advice,
  submission,
  isEditor,
  onChange,
}: {
  advice: Advice;
} & Pick<WorkspaceProps, 'submission' | 'isEditor' | 'onChange'>) {
  return (
    <div className="report">
      <h3>Contributor advice</h3>
      <Markdown text={advice.summary} />
      <p className="small muted">
        Checked items include the adviser’s evidence. Proposed items still need checking.
      </p>
      {advice.improvements.length > 0 ? (
        <table className="data-table referee-table">
          <caption className="visually-hidden">Suggested improvements</caption>
          <thead>
            <tr>
              <th>Kind</th>
              <th>Statement</th>
              <th>Suggestion</th>
              <th>How we help</th>
              <th>Effort</th>
              <th>Status and evidence</th>
            </tr>
          </thead>
          <tbody>
            {advice.improvements.map((item, i) => (
              <tr key={i}>
                <td data-label="Kind">{IMPROVEMENT_KIND_LABELS[item.kind]}</td>
                <td data-label="Statement">
                  <StatementRef id={item.claim} submission={submission} />
                </td>
                <td data-label="Suggestion">
                  <Markdown text={item.suggestion} />
                </td>
                <td data-label="How we help">
                  <Markdown text={item.how_we_help} />
                </td>
                <td data-label="Effort">{EFFORT_LABELS[item.effort]}</td>
                <td data-label="Status and evidence">
                  <Badge tone={item.status === 'checked' ? 'good' : 'warn'}>
                    {EVIDENCE_LABELS[item.status]}
                  </Badge>
                  {item.evidence ? <Markdown text={item.evidence} /> : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {advice.formalization.length > 0 ? (
        <>
          <h4>Formalization candidates</h4>
          <ul className="formal-list">
            {advice.formalization.map((candidate, i) => (
              <li key={i}>
                <CandidateView
                  candidate={candidate}
                  submission={submission}
                  isEditor={isEditor}
                  onChange={onChange}
                />
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}

function CandidateView({
  candidate,
  submission,
  isEditor,
  onChange,
}: { candidate: FormalizationCandidate } & Pick<
  WorkspaceProps,
  'submission' | 'isEditor' | 'onChange'
>) {
  const api = useApi();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const claim = submission.claims.find((claim) => claim.id === candidate.claim);
  const canPropose =
    isEditor &&
    submission.status.state === 'accepted' &&
    claim &&
    !isOpenKind(claim.kind) &&
    !submission.formalization.items.some((item) => item.claim === candidate.claim);
  const propose = async () => {
    if (candidate.plan.trim() === '') {
      setError('Say why this statement is worth formalizing.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      onChange(await api.proposeFormalization(submission.id, candidate.claim, candidate.plan));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div>
      <div className="claim-head">
        <StatementRef id={candidate.claim} submission={submission} />
        <Badge tone={candidate.feasibility === 'ready' ? 'accent' : 'muted'}>
          {FEASIBILITY_LABELS[candidate.feasibility]}
        </Badge>
        <span className="small">Effort: {EFFORT_LABELS[candidate.effort]}</span>
      </div>
      <h5>Mathlib notions</h5>
      {candidate.mathlib.length > 0 ? (
        <MarkdownList items={candidate.mathlib} />
      ) : (
        <p className="muted">None named.</p>
      )}
      <h5>Missing pieces</h5>
      {candidate.missing.length > 0 ? (
        <MarkdownList items={candidate.missing} />
      ) : (
        <p className="muted">None named.</p>
      )}
      <h5>Lean sketch</h5>
      <div className="code-block">
        <pre>
          <code>{candidate.lean_sketch}</code>
        </pre>
      </div>
      <h5>Plan</h5>
      <Markdown text={candidate.plan} />
      {canPropose ? (
        <button
          type="button"
          className="button button-quiet button-small"
          disabled={busy}
          onClick={() => void propose()}
        >
          Propose formalization
        </button>
      ) : null}
      <InlineError error={error} />
    </div>
  );
}

function ProbeView({ probe, submission }: { probe: FormalProbe; submission: Submission }) {
  return (
    <div className="report">
      <h3>Lean formalization probe</h3>
      <p className="small muted">
        Private · {probe.toolchain} · check that each Lean statement matches the paper
      </p>
      {probe.summary ? <Markdown text={probe.summary} /> : null}
      <ul className="formal-list">
        {probe.attempts.map((attempt) => (
          <li key={attempt.claim}>
            <div className="claim-head">
              <StatementRef id={attempt.claim} submission={submission} />
              <Badge tone={attempt.outcome === 'compiled' ? 'good' : 'warn'}>
                {PROBE_OUTCOME_LABELS[attempt.outcome]}
              </Badge>
              {attempt.theorem ? <code className="small">{attempt.theorem}</code> : null}
            </div>
            {attempt.outcome === 'compiled' ? (
              <p className="small">
                Axioms: {(attempt.axioms?.length ?? 0) > 0 ? attempt.axioms?.join(', ') : 'none'}
              </p>
            ) : null}
            {attempt.note ? <Markdown text={attempt.note} /> : null}
            {attempt.log ? (
              <details className="history">
                <summary>Why the check failed</summary>
                <div className="code-block">
                  <pre>
                    <code>{attempt.log}</code>
                  </pre>
                </div>
              </details>
            ) : null}
            {attempt.lean ? (
              <details className="history">
                <summary>Lean source</summary>
                <div className="code-block">
                  <pre>
                    <code>{attempt.lean}</code>
                  </pre>
                </div>
              </details>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

function LetterForm({
  draft,
  recommendation,
  submissionId,
  onSent,
}: {
  draft: LetterDraft;
  recommendation?: Recommendation;
  submissionId: string;
  onSent: (letter: FeedbackLetter) => void;
}) {
  const api = useApi();
  const [subject, setSubject] = useState(draft.subject);
  const [body, setBody] = useState(draft.body);
  const [note, setNote] = useState(draft.note);
  const [assessment, setAssessment] = useState<Recommendation | ''>(recommendation ?? '');
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (body.trim() === '') {
      setError('The letter body is empty. Write feedback before sending it to the author.');
      return;
    }
    setBusy(true);
    setSent(false);
    setError(null);
    try {
      onSent(
        await api.sendFeedback(submissionId, {
          subject,
          body,
          note,
          ...(assessment ? { assessment } : {}),
        }),
      );
      setSent(true);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      className="form"
      aria-label="Send feedback to the author"
      onSubmit={(event) => void submit(event)}
    >
      <Field label="Assessment" htmlFor="feedback-assessment">
        <select
          id="feedback-assessment"
          value={assessment}
          onChange={(e) => setAssessment(e.target.value as Recommendation | '')}
          disabled={busy}
        >
          <option value="accept">Accept</option>
          <option value="minor_revision">Minor revision</option>
          <option value="major_revision">Major revision</option>
          <option value="reject">Reject</option>
          <option value="">No assessment</option>
        </select>
      </Field>
      <Field label="Subject" htmlFor="feedback-subject">
        <input
          id="feedback-subject"
          value={subject}
          onChange={(e) => setSubject(e.target.value)}
          disabled={busy}
          required
        />
      </Field>
      <Field label="Body" htmlFor="feedback-body" hint="Markdown with LaTeX; sent to the author.">
        <textarea
          id="feedback-body"
          rows={10}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          disabled={busy}
          aria-invalid={body.trim() === '' && error !== null}
        />
      </Field>
      <Field
        label="Note"
        htmlFor="feedback-note"
        hint="Also shared with the author, in an expandable block."
      >
        <textarea
          id="feedback-note"
          rows={4}
          value={note}
          onChange={(e) => setNote(e.target.value)}
          disabled={busy}
        />
      </Field>
      <details className="history">
        <summary>Preview</summary>
        <div className="preview" role="region" aria-label="Feedback preview">
          <h4>Body preview</h4>
          <Markdown text={body} lineBreaks />
          {note.trim() ? (
            <>
              <h4>Note preview</h4>
              <Markdown text={note} />
            </>
          ) : null}
        </div>
      </details>
      <div className="button-row">
        <button type="submit" className="button" disabled={busy}>
          {busy ? 'Sending…' : 'Send to author'}
        </button>
      </div>
      <InlineError error={error} />
      {sent ? <p role="status">Feedback sent to the author.</p> : null}
    </form>
  );
}

function LetterText({ letter }: { letter: LetterDraft }) {
  return (
    <>
      <p>
        <strong>{letter.subject}</strong>
      </p>
      <Markdown text={letter.body} lineBreaks />
      {letter.note ? (
        <details className="history">
          <summary>Note</summary>
          <Markdown text={letter.note} />
        </details>
      ) : null}
    </>
  );
}

/** The opening paragraphs of a letter, with the rest one click away. */
function LetterBody({ body, note }: { body: string; note: string }) {
  const [full, setFull] = useState(false);
  const paragraphs = body.split(/\n\s*\n/);
  const short = paragraphs.length > 3 && !full;
  return (
    <>
      <Markdown text={short ? paragraphs.slice(0, 2).join('\n\n') : body} lineBreaks />
      {short ? (
        <button
          type="button"
          className="button button-quiet button-small letter-more"
          onClick={() => setFull(true)}
        >
          Read the full letter
        </button>
      ) : null}
      {note && !short ? (
        <details className="history">
          <summary>Note</summary>
          <Markdown text={note} />
        </details>
      ) : null}
    </>
  );
}

function SentLetters({
  letters,
  authorView = false,
  showSender = false,
}: {
  letters: FeedbackLetter[];
  authorView?: boolean;
  showSender?: boolean;
}) {
  const newestFirst = [...letters].sort((a, b) => Date.parse(b.sent_at) - Date.parse(a.sent_at));
  return (
    <ul className="referee-letters">
      {newestFirst.map((letter, i) => (
        <li className="report" key={`${letter.sent_at}-${i}`}>
          {authorView ? (
            <>
              <h3>
                {letter.subject}
                {letter.assessment ? (
                  <>
                    {' '}
                    <AssessmentBadge assessment={letter.assessment} />
                  </>
                ) : null}
              </h3>
              <p className="entry-meta">
                <DateText iso={letter.sent_at} withTime />
                {showSender ? <span>Sent by {letter.sent_by}</span> : null}
                {showSender && letter.edited ? <Badge tone="muted">Edited</Badge> : null}
              </p>
              <LetterBody body={letter.body} note={letter.note} />
            </>
          ) : (
            <details className="history">
              <summary>
                <strong>{letter.subject}</strong>
                {letter.assessment ? (
                  <>
                    {' '}
                    <AssessmentBadge assessment={letter.assessment} />
                  </>
                ) : null}
                {' · '}
                <DateText iso={letter.sent_at} withTime />
                {' · Sent by '}
                <span>{letter.sent_by}</span>
                {letter.edited ? (
                  <>
                    {' '}
                    <Badge tone="muted">Edited</Badge>
                  </>
                ) : null}
              </summary>
              <Markdown text={letter.body} lineBreaks />
              {letter.note ? (
                <details className="history">
                  <summary>Note</summary>
                  <Markdown text={letter.note} />
                </details>
              ) : null}
            </details>
          )}
        </li>
      ))}
    </ul>
  );
}

function AssessmentBadge({ assessment }: { assessment: Recommendation }) {
  const label = RECOMMENDATION_LABELS[assessment];
  return (
    <Badge tone={assessment === 'accept' ? 'good' : assessment === 'reject' ? 'bad' : 'warn'}>
      {label[0]!.toUpperCase() + label.slice(1)}
    </Badge>
  );
}
