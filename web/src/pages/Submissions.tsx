import { useCallback } from 'react';
import type { SubmissionScope } from '../api/client';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { usePaged } from '../api/usePaged';
import { hasRole, useSession } from '../auth/session';
import { SubmissionStatusBadge } from '../components/badges';
import { Pager } from '../components/Pager';
import { Async, DateText, Loading, SignInPrompt } from '../components/ui';
import { formatAuthors, SUBMISSION_KIND_LABELS, newResults } from '../lib/labels';
import { Link } from '../routing/router';

const COPY: { [K in SubmissionScope]: { title: string; lede: string; empty: string } } = {
  mine: {
    title: 'My work',
    lede: 'Papers, short notes and conjectures you have submitted, newest first. A draft waits for you to confirm its statements.',
    empty: 'You have not submitted work yet.',
  },
  queue: {
    title: 'Review queue',
    lede: 'Papers in review and drafts, for editors, reviewer accounts and admins.',
    empty: 'The queue is empty.',
  },
};

function newResultCount(s: import('../api/types').Submission): number {
  const reports = s.reports.filter((r) => r.claims_revision === s.claims_revision).reverse();
  const escape = reports.find((r) => r.payload.stage === 'escape')?.payload;
  const literature = reports.find((r) => r.payload.stage === 'literature')?.payload;
  if (escape?.stage !== 'escape') return 0;
  return s.claims.filter(
    (c) =>
      c.role === 'main' &&
      c.has_proof &&
      c.kind !== 'conjecture' &&
      c.kind !== 'question' &&
      !(literature?.stage === 'literature' ? literature.prior : []).some(
        (p) => p.claim === c.id && p.relation !== 'related',
      ) &&
      escape.assessments.some(
        (a) =>
          a.claim === c.id &&
          a.correctness === 'correct' &&
          a.shape === 'content' &&
          a.witnesses.length > 0,
      ),
  ).length;
}

export function SubmissionsPage({ scope }: { scope: SubmissionScope }) {
  const { session, person } = useSession();
  const api = useApi();
  const allowed =
    scope === 'mine' ? person !== null : hasRole(person, 'editor', 'reviewer', 'admin');
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listSubmissions(scope, { before, limit: 20 }, { signal }),
    [api, scope],
  );
  const paged = usePaged(allowed ? fetchPage : null);
  const copy = COPY[scope];
  const loadNotifications = useCallback(
    (signal: AbortSignal) => api.notifications({ signal }),
    [api],
  );
  const notifications = useAsync(scope === 'mine' && person ? loadNotifications : null);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{copy.title}</h1>
          <p className="lede">{copy.lede}</p>
        </div>
        {scope === 'mine' && person ? (
          <Link to={{ kind: 'submit' }} className="button">
            Submit work
          </Link>
        ) : null}
      </header>
      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what={`see ${copy.title.toLowerCase()}`} />
      ) : null}
      {person !== null && !allowed ? (
        <p className="notice notice-info">
          The review queue is for editors, reviewer accounts and admins.
        </p>
      ) : null}
      {notifications.state.status === 'ok' && notifications.state.value.length ? (
        <section aria-label="Conjecture solutions">
          <h2>Your conjectures have answers</h2>
          <ul className="entry-list">
            {notifications.state.value.map((n) => (
              <li key={n.attempt}>
                <Link to={{ kind: 'conjecture', record: n.record, claim: n.claim }}>
                  {n.record} · {n.claim}
                </Link>
                <p>
                  {n.entrant.name} submitted a verified answer. <DateText iso={n.at} />
                </p>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      <Async state={paged.state} onRetry={paged.reload}>
        {(listing) =>
          listing.items.length === 0 ? (
            <p className="muted">{copy.empty}</p>
          ) : (
            <ul className="entry-list">
              {listing.items.map((s) => (
                <li key={s.id}>
                  <Link to={{ kind: 'submission', id: s.id }} className="entry-title">
                    {s.title}
                  </Link>
                  <p className="entry-meta">
                    <span>{SUBMISSION_KIND_LABELS[s.kind]}</span>
                    <SubmissionStatusBadge status={s.status} />
                    <span>{formatAuthors(s.authors)}</span>
                    {newResultCount(s) > 0 ? <span>{newResults(newResultCount(s))}</span> : null}
                    {s.formalization.items.some((i) => i.state.state === 'verified') ? (
                      <span>
                        Lean ✓{' '}
                        {s.formalization.items.filter((i) => i.state.state === 'verified').length}
                      </span>
                    ) : null}
                    <span>· version {s.versions.length}</span>
                    <span>
                      · updated <DateText iso={s.updated_at} />
                    </span>
                    {s.status.state === 'accepted' ? (
                      <span>
                        · <code>{s.status.record}</code>
                      </span>
                    ) : null}
                  </p>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <Pager paged={paged} />
    </div>
  );
}
