import { useCallback } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { Async } from '../components/ui';
import { Link } from '../routing/router';
import { PaperEntry } from './Papers';

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

  return (
    <div className="page home">
      <section className="intro">
        <h1>Bring your work. We help it go further.</h1>
        <p className="lede">A mathematics venue that referees, formalizes and writes back.</p>
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

      <section aria-labelledby="recent-heading" className="featured">
        <h2 id="recent-heading">Recently displayed</h2>
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
