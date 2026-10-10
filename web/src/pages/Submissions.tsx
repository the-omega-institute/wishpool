import { useCallback } from 'react';
import type { SubmissionScope } from '../api/client';
import { useApi } from '../api/context';
import { usePaged } from '../api/usePaged';
import { hasRole, useSession } from '../auth/session';
import { SubmissionStatusBadge } from '../components/badges';
import { Pager } from '../components/Pager';
import { Async, DateText, Loading, SignInPrompt } from '../components/ui';
import { formatAuthors, SUBMISSION_KIND_LABELS } from '../lib/labels';
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
                    <span>
                      · version {s.versions.length} · {s.claims.length || s.extracted.length}{' '}
                      statements
                    </span>
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
