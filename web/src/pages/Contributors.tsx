import { useCallback } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { useSession } from '../auth/session';
import { Async } from '../components/ui';
import { formatCount } from '../lib/format';
import { shortId } from '../lib/labels';
import { Link } from '../routing/router';

export function ContributorsPage() {
  const api = useApi();
  const { person } = useSession();
  const load = useCallback(
    (signal: AbortSignal) => api.listContributors(undefined, { signal }),
    [api],
  );
  const credits = useAsync(load);
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Contributors</h1>
          <p className="lede">
            What each contributor has submitted and what was verified. An escape judgement is
            verified when an editor confirms it or a judgement from a different model family and
            account agrees; a literature check or probe when an editor accepts it; a formalization
            when an editor records the verified Lean proof.
          </p>
          <p className="muted small">
            Metered tokens were measured by NyxID on donated quota. Self-reported tokens are what
            contributors’ own agents reported. The two are kept apart and never added together.{' '}
            <Link to={{ kind: 'contribute' }}>How to contribute →</Link>
          </p>
        </div>
      </header>
      <Async state={credits.state} onRetry={credits.reload}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="muted">No contributions recorded yet.</p>
          ) : (
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th scope="col">Contributor</th>
                    <th scope="col" className="num">
                      Verified
                    </th>
                    <th scope="col" className="num">
                      Submitted
                    </th>
                    <th scope="col" className="num">
                      Rejected
                    </th>
                    <th scope="col" className="num">
                      Verified formalizations
                    </th>
                    <th scope="col" className="num">
                      Metered tokens
                    </th>
                    <th scope="col" className="num">
                      Self-reported tokens
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((c) => (
                    <tr key={c.contributor}>
                      <th scope="row">
                        <code title={c.contributor}>{shortId(c.contributor)}</code>
                        {person?.id === c.contributor ? (
                          <span className="muted small"> (you)</span>
                        ) : null}
                      </th>
                      <td className="num">{formatCount(c.verified)}</td>
                      <td className="num">{formatCount(c.submitted)}</td>
                      <td className="num">{formatCount(c.rejected)}</td>
                      <td className="num">{formatCount(c.verified_formalizations)}</td>
                      <td className="num">{formatCount(c.metered_tokens)}</td>
                      <td className="num">{formatCount(c.reported_tokens)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </Async>
    </div>
  );
}
