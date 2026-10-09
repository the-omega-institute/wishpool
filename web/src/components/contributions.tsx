import { useCallback, useState } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import type { Contribution, Person } from '../api/types';
import { hasRole } from '../auth/session';
import { formatCount } from '../lib/format';
import { shortId } from '../lib/labels';
import { safeHttpUrl } from '../lib/links';
import { CONTRIBUTION_STATE_LABELS, MODE_LABELS, isReviewedKind } from '../lib/tasks';
import { PriorWorkList } from './Analysis';
import { ShapeBadge } from './badges';
import { WitnessList } from './claims';
import { Markdown } from './Markdown';
import { Async, Badge, DateText, ExternalLink, InlineError, type BadgeTone } from './ui';

const CONTRIBUTION_TONES: { [K in Contribution['status']['state']]: BadgeTone } = {
  submitted: 'neutral',
  verified: 'good',
  rejected: 'bad',
};

export function ContributionList({ task, person }: { task: string; person: Person | null }) {
  const api = useApi();
  const load = useCallback(
    (signal: AbortSignal) => api.listContributions({ task, limit: 50 }, { signal }),
    [api, task],
  );
  const listing = useAsync(load);
  return (
    <div className="contributions">
      <Async state={listing.state} onRetry={listing.reload}>
        {(l) =>
          l.items.length === 0 ? (
            <p className="muted small">No contributions yet.</p>
          ) : (
            <ul className="assessment-list">
              {l.items.map((c) => (
                <li key={c.id}>
                  <ContributionView initial={c} person={person} />
                </li>
              ))}
            </ul>
          )
        }
      </Async>
    </div>
  );
}

export function ContributionView({
  initial,
  person,
}: {
  initial: Contribution;
  person: Person | null;
}) {
  const [c, setC] = useState(initial);
  const canReview =
    hasRole(person, 'editor') &&
    person?.id !== c.contributor &&
    isReviewedKind(c.kind) &&
    c.status.state === 'submitted';
  return (
    <div className="contribution">
      <div className="report-head">
        <Badge tone={CONTRIBUTION_TONES[c.status.state]}>
          {CONTRIBUTION_STATE_LABELS[c.status.state]}
        </Badge>
        <span className="reviewer">
          <code title={c.contributor}>{shortId(c.contributor)}</code>
          {person?.id === c.contributor ? <span className="muted"> (you)</span> : null}
        </span>
        <span className="muted">
          {c.agent.tool} · {c.agent.model} · {MODE_LABELS[c.mode]}
        </span>
        <span className="muted">
          <DateText iso={c.submitted_at} withTime />
        </span>
      </div>
      {c.status.state === 'verified' ? <p className="small">{c.status.detail}</p> : null}
      {c.status.state === 'rejected' ? <p className="report-reason">{c.status.reason}</p> : null}
      <OutputView contribution={c} />
      {c.tokens ? (
        <p className="muted small">
          {formatCount(c.tokens.input)} input + {formatCount(c.tokens.output)} output tokens,{' '}
          {c.tokens.metered ? 'metered by NyxID' : 'self-reported'}
        </p>
      ) : null}
      {canReview ? <ReviewForm contribution={c} onChange={setC} /> : null}
    </div>
  );
}

function OutputView({ contribution }: { contribution: Contribution }) {
  const o = contribution.output;
  switch (o.output) {
    case 'judgement':
      return (
        <div>
          <ShapeBadge shape={o.shape} />
          <WitnessList witnesses={o.witnesses} />
          {o.rationale ? <Markdown text={o.rationale} className="rationale" /> : null}
        </div>
      );
    case 'literature':
      return (
        <div>
          {o.summary ? <Markdown text={o.summary} /> : null}
          {o.searched.length > 0 ? (
            <p className="muted small">Searched: {o.searched.join(' · ')}</p>
          ) : null}
          {o.prior.length > 0 ? (
            <PriorWorkList prior={o.prior} />
          ) : (
            <p className="small">No prior work found.</p>
          )}
        </div>
      );
    case 'probe_note':
      return <Markdown text={o.note} />;
    case 'pull_request': {
      const href = safeHttpUrl(o.url);
      return (
        <p>
          Pull request:{' '}
          {href ? <ExternalLink href={href}>{o.url}</ExternalLink> : <code>{o.url}</code>}
        </p>
      );
    }
  }
}

function ReviewForm({
  contribution,
  onChange,
}: {
  contribution: Contribution;
  onChange: (c: Contribution) => void;
}) {
  const api = useApi();
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const review = (accept: boolean) => {
    if (!accept && note.trim() === '') {
      setError('A rejection gives its reason in the note.');
      return;
    }
    setBusy(true);
    setError(null);
    api.reviewContribution(contribution.id, { accept, note: note.trim() }).then(
      (c) => {
        setBusy(false);
        onChange(c);
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };
  const id = `review-${contribution.id}`;
  return (
    <div className="inline-form">
      <div className="field">
        <label htmlFor={id}>Editor’s note</label>
        <input id={id} value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
      <div className="button-row">
        <button
          type="button"
          className="button button-small"
          disabled={busy}
          onClick={() => review(true)}
        >
          Accept
        </button>
        <button
          type="button"
          className="button button-quiet button-small"
          disabled={busy}
          onClick={() => review(false)}
        >
          Reject
        </button>
      </div>
      <InlineError error={error} />
    </div>
  );
}
