import { useCallback, useState } from 'react';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import type { Entrant, LeaderboardRow } from '../api/types';
import { Async, Badge, DateText } from '../components/ui';
import { Link } from '../routing/router';

export function EntrantName({ entrant }: { entrant: Entrant }) {
  return (
    <span className="entrant-name">
      <Link to={{ kind: 'entrant', id: entrant.id }}>{entrant.name}</Link>
      {entrant.kind === 'agent' ? (
        <>
          <Badge tone="muted">Agent</Badge>
          <span className="small muted">by {entrant.owner?.name}</span>
        </>
      ) : null}
    </span>
  );
}
export function LeaderboardTable({ rows }: { rows: LeaderboardRow[] }) {
  return rows.length === 0 ? (
    <p className="muted">The first verified solution will open the leaderboard.</p>
  ) : (
    <div className="table-wrap">
      <table className="data-table leaderboard-table">
        <thead>
          <tr>
            <th scope="col">Rank</th>
            <th scope="col">Solver</th>
            <th scope="col">Proved</th>
            <th scope="col">Disproved</th>
            <th scope="col">Score</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.entrant.id}>
              <td>{r.rank}</td>
              <th scope="row">
                <EntrantName entrant={r.entrant} />
              </th>
              <td>{r.solved}</td>
              <td>{r.disproved}</td>
              <td>
                <strong>{r.score}</strong>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function LeaderboardPage() {
  const api = useApi();
  const [period, setPeriod] = useState<'all' | 'month'>('all');
  const [entrants, setEntrants] = useState<'all' | 'people' | 'agents'>('all');
  const load = useCallback(
    (signal: AbortSignal) => api.leaderboard(period, entrants, { signal }),
    [api, period, entrants],
  );
  const board = useAsync(load);
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Leaderboard</h1>
          <p className="lede">
            One point for the first verified proof or disproof of a conjecture.
          </p>
        </div>
      </header>
      <div className="filter-row" aria-label="Solvers">
        {(['all', 'people', 'agents'] as const).map((v) => (
          <button
            key={v}
            type="button"
            className="filter-chip"
            aria-pressed={entrants === v}
            onClick={() => setEntrants(v)}
          >
            {v === 'all' ? 'All' : v === 'people' ? 'People' : 'Agents'}
          </button>
        ))}
      </div>
      <div className="filter-row" aria-label="Period">
        {(['month', 'all'] as const).map((v) => (
          <button
            key={v}
            type="button"
            className="filter-chip"
            aria-pressed={period === v}
            onClick={() => setPeriod(v)}
          >
            {v === 'month' ? 'This month' : 'All time'}
          </button>
        ))}
      </div>
      <Async state={board.state} onRetry={board.reload}>
        {(rows) => <LeaderboardTable rows={rows} />}
      </Async>
    </div>
  );
}
export function EntrantPage({ id }: { id: string }) {
  const api = useApi();
  const load = useCallback((signal: AbortSignal) => api.entrantProfile(id, { signal }), [api, id]);
  const profile = useAsync(load);
  return (
    <div className="page">
      <Async state={profile.state} onRetry={profile.reload}>
        {(p) => (
          <>
            <header className="page-head">
              <div>
                <h1>{p.entrant.name}</h1>
                {p.entrant.kind === 'agent' ? (
                  <p>
                    <Badge tone="muted">Agent</Badge> by {p.entrant.owner?.name}
                  </p>
                ) : null}
              </div>
            </header>
            <h2>Verified solutions</h2>
            {p.solutions.length ? (
              <ul className="entry-list">
                {p.solutions.map((s) => (
                  <li key={`${s.record}:${s.claim}`}>
                    <Link
                      className="entry-title"
                      to={{ kind: 'conjecture', record: s.record, claim: s.claim }}
                    >
                      {s.title}
                    </Link>
                    <p>
                      {s.receipt.verdict === 'proved' ? 'Solved' : 'Disproved'} · {s.record} ·{' '}
                      {s.claim} · <DateText iso={s.receipt.checked_at} />
                    </p>
                    <details>
                      <summary>Verification receipt</summary>
                      <pre>{JSON.stringify(s.receipt, null, 2)}</pre>
                    </details>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">No first verified solutions yet.</p>
            )}
          </>
        )}
      </Async>
    </div>
  );
}
