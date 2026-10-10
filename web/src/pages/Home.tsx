import { useCallback } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { Async } from '../components/ui';
import { Link } from '../routing/router';
import { PaperEntry } from './Papers';
import { ConjectureEntry } from './Conjectures';
import { LeaderboardTable } from './Leaderboard';

const FLOW = [
  ['Submit', 'A paper, a short note or a conjecture.'],
  ['Referee', 'A reading of the confirmed statements.'],
  ['Audit', 'Check the report against the source.'],
  ['Decision', 'Accepted work appears at once when public.'],
  ['Letter', 'Private feedback and sharpening suggestions.'],
  ['Lean', 'Checked proofs or an author-confirmed conjecture statement.'],
] as const;

export function HomePage() {
  const api = useApi();
  const loadPapers = useCallback(
    (signal: AbortSignal) => api.listPapers({ limit: 5 }, { signal }),
    [api],
  );
  const papers = useAsync(loadPapers);
  const loadConjectures = useCallback(
    (signal: AbortSignal) => api.listConjectures({ limit: 5, status: 'open' }, { signal }),
    [api],
  );
  const conjectures = useAsync(loadConjectures);
  const loadBoard = useCallback(
    (signal: AbortSignal) => api.leaderboard('all', 'all', { signal }),
    [api],
  );
  const board = useAsync(loadBoard);

  return (
    <div className="page home">
      <section className="intro">
        <h1>Open questions. Verified answers.</h1>
        <p className="lede">Read new mathematics. Solve a conjecture. Prove it in Lean.</p>
        <div className="intro-actions">
          <Link to={{ kind: 'submit' }} className="button">
            Submit work
          </Link>
          <Link to={{ kind: 'papers' }} className="button button-quiet">
            Accepted work
          </Link>
        </div>
      </section>

      <ol className="flow" aria-label="How it works">
        {FLOW.map(([title, line], i) => (
          <li key={title}>
            <span className="flow-n">{i + 1}</span>
            <strong>{title}</strong>
            <span>{line}</span>
          </li>
        ))}
      </ol>

      <section aria-labelledby="open-heading">
        <h2 id="open-heading">Open conjectures</h2>
        <Async state={conjectures.state} onRetry={conjectures.reload}>
          {(listing) => {
            const open = listing.items.filter((c) => c.status === 'open').slice(0, 5);
            return open.length ? (
              <ul className="entry-list">
                {open.map((c) => (
                  <li key={`${c.record}:${c.claim}`}>
                    <ConjectureEntry conjecture={c} />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">
                The next open question could be yours.{' '}
                <Link to={{ kind: 'submit' }}>Submit a conjecture</Link>
              </p>
            );
          }}
        </Async>
      </section>
      <section aria-labelledby="board-heading">
        <h2 id="board-heading">Leaderboard</h2>
        <Async state={board.state} onRetry={board.reload}>
          {(rows) => <LeaderboardTable rows={rows.slice(0, 5)} />}
        </Async>
        <p>
          <Link to={{ kind: 'leaderboard' }}>View the leaderboard</Link>
        </p>
      </section>
      <section aria-labelledby="recent-heading" className="featured">
        <h2 id="recent-heading">Recently accepted</h2>
        <Async state={papers.state} onRetry={papers.reload}>
          {(listing) =>
            listing.items.length === 0 ? (
              <p className="muted">None yet.</p>
            ) : (
              <ul className="entry-list">
                {listing.items.map((p) => (
                  <li key={p.record}>
                    <PaperEntry paper={p} />
                  </li>
                ))}
              </ul>
            )
          }
        </Async>
      </section>
    </div>
  );
}
