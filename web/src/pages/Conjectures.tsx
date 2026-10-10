import { useCallback, useState } from 'react';
import { useApi } from '../api/context';
import { usePaged } from '../api/usePaged';
import type { ConjectureSummary } from '../api/types';
import { LatexText } from '../components/Markdown';
import { Pager } from '../components/Pager';
import { Async, Badge, DateText } from '../components/ui';
import { Link } from '../routing/router';
import { EntrantName } from './Leaderboard';
import { formatAuthors, SUBMISSION_KIND_LABELS } from '../lib/labels';
export function ConjectureEntry({ conjecture: c }: { conjecture: ConjectureSummary }) {
  return (
    <article>
      <Link className="entry-title" to={{ kind: 'conjecture', record: c.record, claim: c.claim }}>
        {c.title}
      </Link>
      <p className="entry-meta">
        <code>{c.record}</code>
        <span>{c.claim}</span>
        <span>{SUBMISSION_KIND_LABELS[c.kind ?? 'conjecture']}</span>
        <span>{c.source}</span>
        <Badge tone={c.status === 'open' ? 'muted' : 'good'}>
          {c.status === 'solved' ? 'Solved' : c.status === 'disproved' ? 'Disproved' : 'Open'}
        </Badge>
      </p>
      <p className="entry-meta">
        {c.authors ? <span>{formatAuthors(c.authors)}</span> : null}
        {c.accepted_at ? <DateText iso={c.accepted_at} /> : null}
      </p>
      <LatexText source={c.statement} className="conjecture-one-line" />
      <p className="small muted">
        {c.attempts} attempts ·{' '}
        {c.lean_statement_status === 'confirmed'
          ? 'Target confirmed by author'
          : 'Target awaiting author confirmation'}
      </p>
      {c.solver ? <EntrantName entrant={c.solver} /> : null}
    </article>
  );
}
export function ConjecturesPage() {
  const api = useApi();
  const [status, setStatus] = useState<'open' | 'solved' | 'disproved'>('open');
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listConjectures({ before, limit: 25, status }, { signal }),
    [api, status],
  );
  const paged = usePaged(fetchPage);
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Conjectures</h1>
          <p className="lede">Find an open question. Bring a checked answer.</p>
        </div>
      </header>
      <div className="filter-row" aria-label="Status">
        {(['open', 'solved', 'disproved'] as const).map((v) => (
          <button
            key={v}
            type="button"
            className="filter-chip"
            aria-pressed={status === v}
            onClick={() => setStatus(v)}
          >
            {v === 'open' ? 'Open' : v === 'solved' ? 'Solved' : 'Disproved'}
          </button>
        ))}
      </div>
      <Async state={paged.state} onRetry={paged.reload}>
        {(listing) => {
          const items = listing.items.filter((c) => (c.status ?? 'open') === status);
          return items.length === 0 ? (
            <div className="empty-state">
              <p>No {status} conjectures here yet.</p>
              <Link className="button" to={{ kind: 'submit' }}>
                Submit a conjecture
              </Link>
            </div>
          ) : (
            <ul className="entry-list">
              {items.map((c) => (
                <li key={`${c.record}:${c.claim}`}>
                  <ConjectureEntry conjecture={c} />
                </li>
              ))}
            </ul>
          );
        }}
      </Async>
      <Pager paged={paged} />
    </div>
  );
}
