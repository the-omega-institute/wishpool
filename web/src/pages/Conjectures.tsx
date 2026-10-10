import { useCallback } from 'react';
import { useApi } from '../api/context';
import { usePaged } from '../api/usePaged';
import { LatexText } from '../components/Markdown';
import { Pager } from '../components/Pager';
import { Async } from '../components/ui';
import { Link } from '../routing/router';

const STATUS = {
  none: 'Lean statement not yet prepared',
  awaiting_author: 'Lean statement awaiting author',
  confirmed: 'Lean statement confirmed by author',
};
export function ConjecturesPage() {
  const api = useApi();
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listConjectures({ before, limit: 25 }, { signal }),
    [api],
  );
  const paged = usePaged(fetchPage);
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Conjectures</h1>
          <p className="lede">Open statements with new mathematical content.</p>
        </div>
      </header>
      <Async state={paged.state} onRetry={paged.reload}>
        {(listing) =>
          listing.items.length === 0 ? (
            <p className="muted">No conjectures are displayed yet.</p>
          ) : (
            <ul className="entry-list">
              {listing.items.map((c) => (
                <li key={c.record}>
                  <Link className="entry-title" to={{ kind: 'paper', record: c.record }}>
                    {c.title}
                  </Link>
                  <p className="entry-meta">
                    <code>{c.record}</code> · Conjecture
                  </p>
                  <LatexText source={c.statement} className="conjecture-one-line" />
                  <p className="small muted">{STATUS[c.lean_statement_status]}</p>
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
