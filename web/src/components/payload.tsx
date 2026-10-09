import type { Claim, Evidence, Stage, StagePayload, StageReport } from '../api/types';
import { RELATION_LABELS, rejectReasonText, reviewerLabel, shortId } from '../lib/labels';
import { safeHttpUrl } from '../lib/links';
import { isMachineProposal, type ReportStanding } from '../lib/reports';
import { ClaimRef, EscapeAssessmentView, StatementList } from './claims';
import { OutcomeBadge } from './badges';
import { Markdown } from './Markdown';
import { Badge, DateText, ExternalLink, SourceLink } from './ui';

export function PayloadView({
  payload,
  claims,
  scope,
}: {
  payload: StagePayload;
  /** The claim set references resolve against (anchors in the page's claims section). */
  claims: readonly Claim[];
  /** Anchor prefix for a claims payload's own list, unique on the page. */
  scope: string;
}) {
  switch (payload.stage) {
    case 'hygiene':
      return (
        <ul className="checks">
          {payload.checks.map((c, i) => (
            <li key={`${c.name}-${i}`} className={c.passed ? 'check-pass' : 'check-fail'}>
              <span className="check-mark" aria-hidden="true">
                {c.passed ? '✓' : '✗'}
              </span>
              <span className="visually-hidden">{c.passed ? 'Passed: ' : 'Failed: '}</span>
              <code>{c.name}</code>
              {c.detail ? <span className="check-detail"> — {c.detail}</span> : null}
            </li>
          ))}
        </ul>
      );
    case 'claims':
      return (
        <details className="history">
          <summary>Confirmed statements ({payload.claims.length})</summary>
          <StatementList claims={payload.claims} scope={scope} />
        </details>
      );
    case 'literature':
      return (
        <div className="literature">
          {payload.searched.length > 0 ? (
            <p className="muted">Searched: {payload.searched.join(' · ')}</p>
          ) : null}
          {payload.prior.length === 0 ? (
            <p>No prior work bearing on the statements was found.</p>
          ) : (
            <ul className="prior-list">
              {payload.prior.map((p, i) => (
                <li key={i}>
                  <div>
                    <ClaimRef id={p.claim} claims={claims} />{' '}
                    <Badge tone={p.relation === 'related' ? 'muted' : 'warn'}>
                      {RELATION_LABELS[p.relation]}
                    </Badge>{' '}
                    <SourceLink source={p.source} />
                  </div>
                  {p.note ? <Markdown text={p.note} /> : null}
                </li>
              ))}
            </ul>
          )}
        </div>
      );
    case 'escape':
      return payload.assessments.length === 0 ? (
        <p className="muted">No assessments.</p>
      ) : (
        <ul className="assessment-list">
          {payload.assessments.map((a, i) => (
            <li key={`${a.claim}-${i}`}>
              <EscapeAssessmentView assessment={a} claims={claims} />
            </li>
          ))}
        </ul>
      );
  }
}

function EvidenceList({ evidence }: { evidence: readonly Evidence[] }) {
  if (evidence.length === 0) return null;
  return (
    <div className="evidence">
      <h5>Evidence</h5>
      <ul>
        {evidence.map((e, i) => {
          const href = safeHttpUrl(e.locator);
          return (
            <li key={i}>
              <span className="muted">{e.kind}</span>{' '}
              {href ? (
                <ExternalLink href={href}>{e.locator}</ExternalLink>
              ) : (
                <code>{e.locator}</code>
              )}
              {e.note ? ` — ${e.note}` : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

const STANDING_TEXT: { [K in ReportStanding]: string } = {
  current: 'Current',
  replaced: 'Replaced by a later report',
  superseded: 'Superseded — filed against an earlier set of statements',
};

export function ReportView({
  report,
  claims,
  humanJudgement,
  standing,
  scope,
  compact = false,
}: {
  report: StageReport;
  claims: readonly Claim[];
  humanJudgement: readonly Stage[];
  standing: ReportStanding;
  scope: string;
  compact?: boolean;
}) {
  const proposal = isMachineProposal(report, humanJudgement);
  // Claims reports carry their own claim set; other reports resolve against the current one.
  const resolveAgainst = report.payload.stage === 'claims' ? report.payload.claims : claims;
  return (
    <article className={`report report-${standing}`}>
      <header className="report-head">
        <OutcomeBadge outcome={report.outcome} proposal={proposal} />
        <span className="reviewer">
          {reviewerLabel(report.reviewer)}
          {report.reviewer.kind === 'human' ? (
            <span className="muted"> ({shortId(report.reviewer.person)})</span>
          ) : null}
        </span>
        <span className="muted">
          <DateText iso={report.filed_at} withTime />
        </span>
        <span className="muted">statements rev. {report.claims_revision}</span>
        {standing !== 'current' ? <Badge tone="muted">{STANDING_TEXT[standing]}</Badge> : null}
      </header>
      {report.outcome.outcome === 'fail' ? (
        <p className="report-reason">{rejectReasonText(report.outcome.reason)}</p>
      ) : null}
      {report.outcome.outcome === 'needs_human' ? (
        <p className="report-question">
          <strong>Open question (for the editors):</strong> {report.outcome.question}
        </p>
      ) : null}
      {proposal ? (
        <p className="hint">
          A machine pass on this stage is a proposal until an editor files a report.
        </p>
      ) : null}
      {report.summary ? <Markdown text={report.summary} className="report-summary" /> : null}
      {compact ? null : (
        <>
          <PayloadView payload={report.payload} claims={resolveAgainst} scope={scope} />
          <EvidenceList evidence={report.evidence} />
        </>
      )}
    </article>
  );
}
