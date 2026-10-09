import { useCallback, useState } from 'react';
import { pdfHref } from '../api/client';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import { usePaged } from '../api/usePaged';
import type { PaperSummary, PublicPaper } from '../api/types';
import { AnalysisView, ConjectureList } from '../components/Analysis';
import { AuthorLine } from '../components/AuthorLine';
import { BasisBadge } from '../components/badges';
import { StatementList } from '../components/claims';
import { LatexText, MacrosProvider } from '../components/Markdown';
import { Pager } from '../components/Pager';
import { Async, Badge, DateText, ExternalLink, MscList } from '../components/ui';
import { paperCitation } from '../lib/citation';
import { AI_USE_DESCRIPTIONS, AI_USE_LABELS, formatAuthors } from '../lib/labels';
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
          · <code>{paper.record}</code>
        </span>
        <span>
          · accepted <DateText iso={paper.accepted_at} />
        </span>
      </p>
      <p className="entry-meta">
        <BasisBadge basis={paper.basis} />
        <span>
          {paper.main_results} main result{paper.main_results === 1 ? '' : 's'}
        </span>
        {paper.lean_verified > 0 ? (
          <Badge tone="good">{paper.lean_verified} Lean verified</Badge>
        ) : null}
        <MscList codes={paper.msc} />
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
            Papers that met the publication threshold: a main result carries new content and is not
            stated or directly implied by prior work, or a main result settles a named open problem.{' '}
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
  const latest = paper.versions[paper.versions.length - 1];
  const repo = paper.formalization_repository ? safeHttpUrl(paper.formalization_repository) : null;
  const doiHref = s.doi ? sourceHref({ kind: 'doi', locator: s.doi }) : null;
  return (
    <MacrosProvider macros={paper.macros}>
      <article className="paper">
        <header className="record-head">
          <p className="eyebrow">
            Record <code className="record-id">{s.record}</code>
          </p>
          <h1>{s.title}</h1>
          <AuthorLine authors={s.authors} />
          <p className="entry-meta">
            <BasisBadge basis={s.basis} />
            <span>
              Accepted <DateText iso={s.accepted_at} />
            </span>
            <MscList codes={s.msc} />
            {s.doi ? (
              <span>
                · {doiHref ? <ExternalLink href={doiHref}>doi:{s.doi}</ExternalLink> : s.doi}
              </span>
            ) : null}
          </p>
          <div className="button-row">
            {latest?.has_pdf ? (
              <a className="button" href={pdfHref(s.submission, latest.number)}>
                PDF (version {latest.number})
              </a>
            ) : null}
            {repo ? <ExternalLink href={repo}>Lean formalization repository</ExternalLink> : null}
          </div>
        </header>

        <section aria-labelledby="abstract-h">
          <h2 id="abstract-h">Abstract</h2>
          <LatexText source={s.abstract_text} className="abstract" />
        </section>

        <section aria-labelledby="ai-h">
          <h2 id="ai-h">AI use</h2>
          <p>
            <Badge tone="neutral" title={AI_USE_DESCRIPTIONS[paper.ai_disclosure.level]}>
              {AI_USE_LABELS[paper.ai_disclosure.level]}
            </Badge>{' '}
            {paper.ai_disclosure.statement}
          </p>
        </section>

        {paper.versions.length > 0 ? (
          <section aria-labelledby="versions-h">
            <h2 id="versions-h">Versions</h2>
            <ul className="version-list">
              {[...paper.versions].reverse().map((v) => (
                <li key={v.number}>
                  <strong>Version {v.number}</strong> · <DateText iso={v.uploaded_at} />
                  {v.has_pdf ? (
                    <>
                      {' '}
                      · <a href={pdfHref(s.submission, v.number)}>PDF</a>
                    </>
                  ) : (
                    <span className="muted"> · no PDF</span>
                  )}
                  {v.note ? <span className="muted"> — {v.note}</span> : null}
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        <section aria-labelledby="statements-h">
          <h2 id="statements-h">Statements</h2>
          <StatementList claims={paper.claims} scope="public" />
        </section>

        {paper.analysis ? (
          <>
            <section aria-labelledby="analysis-h">
              <h2 id="analysis-h">Analysis</h2>
              <p className="muted small">
                Published by the author. Literature (S2) and escape analysis (S3) per statement;
                judgements carry their standing.
              </p>
              <AnalysisView analysis={paper.analysis} claims={paper.claims} scope="public" />
            </section>
            <section aria-labelledby="conjectures-h">
              <h2 id="conjectures-h">Conjecture follow-ups</h2>
              <ConjectureList
                conjectures={paper.conjectures}
                claims={paper.claims}
                scope="public"
              />
            </section>
          </>
        ) : (
          <p className="muted analysis-private">
            The authors keep the per-statement analysis private.
          </p>
        )}

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
