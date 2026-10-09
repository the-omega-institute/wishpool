import { useCallback } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { Async } from '../components/ui';
import { Link } from '../routing/router';
import { PaperEntry } from './Papers';

const FLOW = [
  ['Upload', 'Your LaTeX source, as it is.'],
  ['Referee', 'A full reading of every statement and proof.'],
  ['Lean', 'The statements within reach, formalized and checked.'],
  ['Feedback', 'A letter from the editors, with what we can do together.'],
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
        <h1>Bring your paper. We help it go further.</h1>
        <p className="lede">A mathematics venue that referees, formalizes and writes back.</p>
        <div className="intro-actions">
          <Link to={{ kind: 'submit' }} className="button">
            Submit a paper
          </Link>
          <Link to={{ kind: 'papers' }} className="button button-quiet">
            Accepted papers
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
