import { useState, type FormEvent } from 'react';
import { useApi } from '../../api/context';
import type { Submission } from '../../api/types';
import { useSession } from '../../auth/session';
import { VISIBILITY_LABELS } from '../../lib/labels';
import { SOURCE_ACCEPT, sourceFileProblem } from '../../lib/upload';
import { Link } from '../../routing/router';
import { ItemStateBadge } from '../badges';
import { ClaimRef, StatementView } from '../claims';
import { Field, InlineError } from '../ui';

type OnChange = (s: Submission) => void;

/** Run a mutation that returns the updated submission, tracking busy and error. */
function useMutation(onChange: OnChange) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const run = (call: () => Promise<Submission>, after?: () => void) => {
    setBusy(true);
    setError(null);
    call().then(
      (s) => {
        setBusy(false);
        onChange(s);
        after?.();
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };
  return { busy, error, setError, run };
}

export function ContributorsToggle({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const api = useApi();
  const m = useMutation(onChange);
  const open = submission.open_to_contributors;
  return (
    <div className="setting">
      <label className="checkbox">
        <input
          type="checkbox"
          checked={open}
          disabled={m.busy}
          onChange={(e) => m.run(() => api.setContributors(submission.id, e.target.checked))}
        />
        <span>
          Volunteer contributors may help analyse this paper. While it is in review they see the
          statements, their dependencies, the title and the abstract; after acceptance they may help
          formalize approved statements and probe conjectures. Turning this off closes the paper’s
          open tasks.
        </span>
      </label>
      {open ? (
        <p className="small">
          <Link to={{ kind: 'tasks', submission: submission.id }}>Tasks for this paper</Link>
        </p>
      ) : null}
      <InlineError error={m.error} />
    </div>
  );
}

export function WithdrawPanel({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const api = useApi();
  const m = useMutation(onChange);
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="setting">
      {confirming ? (
        <div className="button-row">
          <span className="small">
            Withdraw the paper? Review stops and the paper stays private.
          </span>
          <button
            type="button"
            className="button button-danger"
            disabled={m.busy}
            onClick={() =>
              m.run(
                () => api.withdrawSubmission(submission.id),
                () => setConfirming(false),
              )
            }
          >
            Withdraw
          </button>
          <button
            type="button"
            className="button button-quiet"
            onClick={() => setConfirming(false)}
          >
            Keep it
          </button>
        </div>
      ) : (
        <button type="button" className="button button-quiet" onClick={() => setConfirming(true)}>
          Withdraw paper…
        </button>
      )}
      <InlineError error={m.error} />
    </div>
  );
}

export function NewVersionForm({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const api = useApi();
  const m = useMutation(onChange);
  const [file, setFile] = useState<File | null>(null);
  const [note, setNote] = useState('');
  const [key, setKey] = useState(0);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (file === null) {
      m.setError('Choose the revised LaTeX source.');
      return;
    }
    const problem = sourceFileProblem(file);
    if (problem) {
      m.setError(problem);
      return;
    }
    if (note.trim() === '') {
      m.setError('Say what changed in this version.');
      return;
    }
    m.run(
      () => api.uploadVersion(submission.id, file, note.trim()),
      () => {
        setFile(null);
        setNote('');
        setKey((k) => k + 1);
      },
    );
  };
  return (
    <form className="tool-form" onSubmit={submit} aria-label="Upload a new version" noValidate>
      <h3>Upload a new version</h3>
      <p className="small">
        The paper returns to draft: the statements are read again from the new source and you
        confirm them again. Reports on the earlier statements no longer count.
      </p>
      <Field
        label="Revised LaTeX source"
        htmlFor="version-file"
        hint=".tex, .zip or .tar.gz, at most 30 MB"
      >
        <input
          key={key}
          id="version-file"
          type="file"
          accept={SOURCE_ACCEPT}
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
        />
      </Field>
      <Field label="What changed" htmlFor="version-note">
        <textarea
          id="version-note"
          rows={2}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </Field>
      <button type="submit" className="button" disabled={m.busy}>
        {m.busy ? 'Uploading…' : 'Upload version'}
      </button>
      <InlineError error={m.error} />
    </form>
  );
}

export function VisibilityChoice({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const api = useApi();
  const m = useMutation(onChange);
  const current = submission.analysis_visibility;
  return (
    <fieldset className="radio-list visibility">
      <legend>Public display</legend>
      <p className="small">
        Currently: <strong>{VISIBILITY_LABELS[current]}</strong>. Reviews and letters are only shown
        to you.
      </p>
      <label className="radio">
        <input
          type="radio"
          name="visibility"
          checked={current === 'public'}
          disabled={m.busy}
          onChange={() => m.run(() => api.setVisibility(submission.id, 'public'))}
        />
        <span>
          <strong>Public</strong> — show statements, dependencies, new lemmas and verified Lean.
        </span>
      </label>
      <label className="radio">
        <input
          type="radio"
          name="visibility"
          checked={current === 'private'}
          disabled={m.busy}
          onChange={() => m.run(() => api.setVisibility(submission.id, 'private'))}
        />
        <span>
          <strong>Private</strong> — show only the title, authors, kind and record.
        </span>
      </label>
      <InlineError error={m.error} />
    </fieldset>
  );
}

/** Formalization items: the author approves or declines each proposal. */
export function FormalizationForAuthor({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const items = submission.formalization.items;
  if (items.length === 0) {
    return <p className="muted">The editors have not proposed any statement for formalization.</p>;
  }
  return (
    <ul className="formal-list">
      {items.map((item) => {
        const claim = submission.claims.find((c) => c.id === item.claim);
        return (
          <li key={item.claim}>
            <div className="claim-head">
              <ClaimRef id={item.claim} claims={submission.claims} scope="ws" />
              <ItemStateBadge state={item.state} />
            </div>
            <p className="small">
              <strong>Editors’ reason:</strong> {item.reason}
            </p>
            {item.state.state === 'declined' ? (
              <p className="small muted">You declined: {item.state.reason}</p>
            ) : null}
            {item.state.state === 'proposed' ? (
              <>
                {claim ? (
                  <details>
                    <summary>Statement</summary>
                    <StatementView claim={claim} claims={submission.claims} scope="proposal" />
                  </details>
                ) : null}
                <FormalizationResponse
                  submission={submission}
                  claim={item.claim}
                  onChange={onChange}
                />
              </>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

function FormalizationResponse({
  submission,
  claim,
  onChange,
}: {
  submission: Submission;
  claim: string;
  onChange: OnChange;
}) {
  const api = useApi();
  const m = useMutation(onChange);
  const [reason, setReason] = useState('');
  const id = `respond-${claim}`;
  return (
    <div className="inline-form">
      <div className="field">
        <label htmlFor={id}>Reason (needed when declining)</label>
        <input id={id} value={reason} onChange={(e) => setReason(e.target.value)} />
      </div>
      <div className="button-row">
        <button
          type="button"
          className="button button-small"
          disabled={m.busy}
          onClick={() =>
            m.run(() =>
              api.respondFormalization(submission.id, claim, {
                approve: true,
                ...(reason.trim() ? { reason: reason.trim() } : {}),
              }),
            )
          }
        >
          Approve formalization
        </button>
        <button
          type="button"
          className="button button-quiet button-small"
          disabled={m.busy}
          onClick={() => {
            if (reason.trim() === '') {
              m.setError('Give a reason for declining.');
              return;
            }
            m.run(() =>
              api.respondFormalization(submission.id, claim, {
                approve: false,
                reason: reason.trim(),
              }),
            );
          }}
        >
          Decline
        </button>
      </div>
      <InlineError error={m.error} />
    </div>
  );
}

export function LeanStatementCard({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: OnChange;
}) {
  const current = submission.versions.at(-1)?.number;
  const claims = submission.claims.filter(
    (c) => c.role === 'main' && (c.kind === 'conjecture' || c.kind === 'question'),
  );
  return (
    <section className="lean-statement-card" aria-label="Lean statement">
      <h2>Is this your conjecture?</h2>
      <p>
        Lean checks that this statement is well formed. Confirm that its meaning matches your
        conjecture.
      </p>
      {claims.map((claim) => {
        const attempt = [...submission.lean_statements]
          .reverse()
          .find(
            (a) =>
              a.claim === claim.id &&
              a.version === current &&
              a.claims_revision === submission.claims_revision,
          );
        return (
          <div key={claim.id}>
            {attempt ? (
              <LeanStatementResponse
                key={attempt.digest}
                submission={submission}
                attempt={attempt}
                onChange={onChange}
              />
            ) : (
              <p role="status">The Lean statement is being prepared.</p>
            )}
          </div>
        );
      })}
    </section>
  );
}

function LeanStatementResponse({
  submission,
  attempt,
  onChange,
}: {
  submission: Submission;
  attempt: Submission['lean_statements'][number];
  onChange: OnChange;
}) {
  const api = useApi();
  const { person } = useSession();
  const canRespond = person?.id === submission.submitter;
  const mutation = useMutation(onChange);
  const [comment, setComment] = useState('');
  const [rejecting, setRejecting] = useState(false);
  const respond = (confirm: boolean) => {
    if (!confirm && !comment.trim()) {
      mutation.setError('Describe what the statement should say.');
      return;
    }
    mutation.run(() =>
      api.respondLeanStatement(submission.id, {
        digest: attempt.digest,
        confirm,
        ...(confirm ? {} : { comment: comment.trim() }),
      }),
    );
  };
  return (
    <div>
      <p>{attempt.reading}</p>
      <pre className="lean-source">
        <code>{attempt.lean}</code>
      </pre>
      {attempt.response.state === 'confirmed' ? (
        <p role="status">
          {canRespond
            ? 'You confirmed this statement.'
            : 'The submitting author confirmed this statement.'}
        </p>
      ) : attempt.response.state === 'rejected' ? (
        <p role="status">A revised statement is being prepared with your correction.</p>
      ) : !canRespond ? (
        <p role="status">Awaiting confirmation from the submitting author.</p>
      ) : (
        <>
          <div className="button-row">
            <button
              type="button"
              className="button"
              disabled={mutation.busy}
              onClick={() => respond(true)}
            >
              Yes, this is my conjecture
            </button>
            <button
              type="button"
              className="button button-quiet"
              disabled={mutation.busy}
              onClick={() => setRejecting(true)}
            >
              No, it should say…
            </button>
          </div>
          {rejecting ? (
            <div className="form">
              <Field label="What should it say?" htmlFor={`lean-correction-${attempt.claim}`}>
                <textarea
                  id={`lean-correction-${attempt.claim}`}
                  rows={3}
                  maxLength={10000}
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                />
              </Field>
              <button
                type="button"
                className="button"
                disabled={mutation.busy}
                onClick={() => respond(false)}
              >
                Request a revised statement
              </button>
            </div>
          ) : null}
        </>
      )}
      <InlineError error={mutation.error} />
    </div>
  );
}
