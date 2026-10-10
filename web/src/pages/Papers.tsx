import { useCallback, useState } from 'react';
import { pdfHref } from '../api/client';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { usePaged } from '../api/usePaged';
import type { PaperSummary, PublicPaper } from '../api/types';
import { AuthorLine } from '../components/AuthorLine';
import { BasisBadge } from '../components/badges';
import { PublicStatements, statementName } from '../components/StatementSummary';
import { LatexText, MacrosProvider } from '../components/Markdown';
import { Pager } from '../components/Pager';
import { Async, Badge, DateText, ExternalLink, MscList } from '../components/ui';
import { paperCitation } from '../lib/citation';
import { CLAIM_KIND_LABELS, SUBMISSION_KIND_LABELS, formatAuthors } from '../lib/labels';
import { safeHttpUrl, sourceHref } from '../lib/links';
import { Link } from '../routing/router';

export function PaperEntry({ paper }: { paper: PaperSummary }) {
  return (
    <article className="paper-entry">
      <p className="entry-title">
        <Link to={{ kind: 'paper', record: paper.record }}>{paper.title}</Link>
      </p>
      <p className="entry-meta">
        <span>{formatAuthors(paper.authors)}</span>
        <span>
          · <code>{paper.record}</code> · {SUBMISSION_KIND_LABELS[paper.kind]}
        </span>
        <span>
          {paper.accepted_at ? (
            <>
              · accepted <DateText iso={paper.accepted_at} />
            </>
          ) : null}
        </span>
      </p>
      <p className="entry-meta">
        {paper.basis ? <BasisBadge basis={paper.basis} /> : null}
        {paper.main_results !== undefined ? (
          <span>
            {paper.main_results} main result{paper.main_results === 1 ? '' : 's'}
          </span>
        ) : null}
        {(paper.lean_verified ?? 0) > 0 ? (
          <Badge tone="good">{paper.lean_verified} Lean verified</Badge>
        ) : null}
        <MscList codes={paper.msc ?? []} />
      </p>
    </article>
  );
}

export function PapersPage() {
  const api = useApi();
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listPapers({ before, limit: 25 }, { signal }),
    [api],
  );
  const paged = usePaged(fetchPage);
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Accepted papers</h1>
          <p className="lede">
            Papers and notes with new mathematical content, and conjectures accepted for display.{' '}
            <Link to={{ kind: 'policy' }}>Review policy</Link>
          </p>
        </div>
      </header>
      <Async state={paged.state} onRetry={paged.reload}>
        {(listing) =>
          listing.items.length === 0 ? (
            <p className="muted">No papers have been accepted yet.</p>
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
      <Pager paged={paged} />
    </div>
  );
}

export function PaperPage({ record }: { record: string }) {
  const api = useApi();
  const load = useCallback(
    (signal: AbortSignal) => api.getPaper(record, { signal }),
    [api, record],
  );
  const paper = useAsync(load);
  return (
    <div className="page">
      <Async state={paper.state} onRetry={paper.reload}>
        {(p) => <PaperView paper={p} />}
      </Async>
    </div>
  );
}

export function PaperView({ paper }: { paper: PublicPaper }) {
  const s = paper.summary;
  const latest = paper.versions?.at(-1);
  const claims = paper.claims ?? [];
  const repo = paper.formalization_repository ? safeHttpUrl(paper.formalization_repository) : null;
  const doiHref = s.doi ? sourceHref({ kind: 'doi', locator: s.doi }) : null;
  return (
    <MacrosProvider macros={paper.macros ?? {}}>
      <article className="paper">
        <header className="record-head">
          <p className="eyebrow">
            Record <code className="record-id">{s.record}</code> · {SUBMISSION_KIND_LABELS[s.kind]}
          </p>
          <h1>{s.title}</h1>
          <AuthorLine authors={s.authors} />
          <p className="entry-meta">
            {s.accepted_at ? (
              <span>
                {s.kind === 'conjecture' ? 'Displayed' : 'Accepted'}{' '}
                <DateText iso={s.accepted_at} />
              </span>
            ) : null}
            <MscList codes={s.msc ?? []} />
            {s.doi ? (
              <span>
                · {doiHref ? <ExternalLink href={doiHref}>doi:{s.doi}</ExternalLink> : s.doi}
              </span>
            ) : null}
          </p>
          <div className="button-row">
            {latest?.has_pdf && s.submission ? (
              <a className="button" href={pdfHref(s.submission, latest.number)}>
                PDF (version {latest.number})
              </a>
            ) : null}
            {repo ? <ExternalLink href={repo}>Lean formalization repository</ExternalLink> : null}
          </div>
        </header>

        {s.abstract_text ? (
          <section aria-labelledby="abstract-h">
            <h2 id="abstract-h">Abstract</h2>
            <LatexText source={s.abstract_text} className="abstract" />
          </section>
        ) : null}
        {paper.new_content?.length ? (
          <section aria-labelledby="new-content-h" className="new-content">
            <h2 id="new-content-h">
              {s.kind === 'note' ? 'New in this note' : 'New in this paper'}
            </h2>
            <ul className="new-content-list">
              {paper.new_content.map((w) => {
                const claim = claims.find((c) => c.id === w.claim);
                return (
                  <li key={w.claim}>
                    <p className="new-content-claim">
                      <span className="statement-kind">
                        {claim ? CLAIM_KIND_LABELS[claim.kind] : 'Statement'}{' '}
                        <span className="statement-id">{w.claim}</span>
                      </span>{' '}
                      {claim ? (
                        <LatexText
                          source={statementName(claim) ?? claim.statement}
                          className="statement-preview"
                        />
                      ) : null}
                    </p>
                    <ul className="new-content-lemmas">
                      {w.lemmas.map((lemma, i) => (
                        <li key={i}>
                          <LatexText source={lemma} />
                        </li>
                      ))}
                    </ul>
                  </li>
                );
              })}
            </ul>
          </section>
        ) : null}
        {claims.length > 0 ? (
          <section aria-labelledby="statements-h" className="statements">
            <h2 id="statements-h">Statements</h2>
            <PublicStatements claims={claims} />
          </section>
        ) : (
          <p className="muted">The author keeps the mathematical details private.</p>
        )}
        {paper.lean_statements?.map((target) => (
          <section key={target.digest} aria-label="Author-confirmed Lean statement">
            <h2>Author-confirmed Lean statement</h2>
            <pre className="lean-source">
              <code>{target.lean}</code>
            </pre>
          </section>
        ))}

        <Citation paper={s} />
      </article>
    </MacrosProvider>
  );
}

function Citation({ paper }: { paper: PaperSummary }) {
  const [copied, setCopied] = useState(false);
  const text = paperCitation(paper, window.location.origin);
  const canCopy = typeof navigator !== 'undefined' && Boolean(navigator.clipboard);
  return (
    <section aria-labelledby="cite-h" className="citation">
      <h2 id="cite-h">Cite</h2>
      <p className="cite">{text}</p>
      {canCopy ? (
        <button
          type="button"
          className="button button-quiet button-small"
          onClick={() =>
            navigator.clipboard.writeText(text).then(
              () => setCopied(true),
              () => setCopied(false),
            )
          }
        >
          {copied ? 'Copied' : 'Copy citation'}
        </button>
      ) : null}
    </section>
  );
}
