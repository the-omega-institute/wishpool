import type { ConjectureFollowUp, PaperAnalysis, PriorWork } from '../api/types';
import { formatDate } from '../lib/format';
import { RELATION_LABELS, SETTLEMENT_LABELS } from '../lib/labels';
import {
  ConjectureStateBadge,
  ItemStateBadge,
  KindBadge,
  RoleBadge,
  ShapeBadge,
  StandingBadge,
} from './badges';
import {
  ClaimRef,
  EscapeAssessmentView,
  WitnessList,
  statementTitle,
  type StatementLike,
} from './claims';
import { Markdown } from './Markdown';
import { Badge, SourceLink } from './ui';

export function PriorWorkList({ prior }: { prior: readonly PriorWork[] }) {
  if (prior.length === 0) return null;
  return (
    <ul className="prior-list">
      {prior.map((p, i) => (
        <li key={i}>
          <Badge tone={p.relation === 'related' ? 'muted' : 'warn'}>
            {RELATION_LABELS[p.relation]}
          </Badge>{' '}
          <SourceLink source={p.source} />
          {p.note ? <Markdown text={p.note} /> : null}
        </li>
      ))}
    </ul>
  );
}

/**
 * The per-statement analysis: S2 (is it known?) and S3 (does it carry new
 * content?), the standing of the escape judgements, and formalization.
 */
export function AnalysisView({
  analysis,
  claims,
  scope,
}: {
  analysis: PaperAnalysis;
  /** Statements to link to (anchors on the same page). */
  claims: readonly StatementLike[];
  scope?: string;
}) {
  return (
    <div className="analysis">
      <dl className="record-meta analysis-counts">
        <div>
          <dt>Main results</dt>
          <dd>{analysis.main_results}</dd>
        </div>
        <div>
          <dt>Main results with new content</dt>
          <dd>{analysis.main_with_content}</dd>
        </div>
        <div>
          <dt>Main results already known</dt>
          <dd>{analysis.main_known}</dd>
        </div>
        <div>
          <dt>Open statements</dt>
          <dd>{analysis.open_statements}</dd>
        </div>
        <div>
          <dt>Lean verified</dt>
          <dd>{analysis.formalized}</dd>
        </div>
      </dl>
      <ol className="analysis-list">
        {analysis.claims.map((c) => (
          <li key={c.claim} className="analysis-item">
            <div className="claim-head">
              <ClaimRef id={c.claim} claims={claims} scope={scope} />
              {claims.some((x) => x.id === c.claim) ? null : (
                <span className="claim-label">{statementTitle({ ...c, id: c.claim })}</span>
              )}
              <KindBadge kind={c.kind} />
              <RoleBadge role={c.role} />
              {c.formalization ? <ItemStateBadge state={c.formalization} /> : null}
            </div>
            <div className="analysis-grid">
              <section aria-label={`Literature for ${c.claim}`}>
                <h5>Literature (S2)</h5>
                {c.known ? (
                  <p>
                    <Badge tone="warn">Known</Badge> Prior work states or directly implies it.
                  </p>
                ) : c.prior.length > 0 ? (
                  <p className="small">No prior work states or directly implies it.</p>
                ) : (
                  <p className="muted small">No prior work recorded.</p>
                )}
                <PriorWorkList prior={c.prior} />
              </section>
              <section aria-label={`Escape analysis for ${c.claim}`}>
                <h5>Escape analysis (S3)</h5>
                {c.assessment ? (
                  <EscapeAssessmentView
                    assessment={c.assessment}
                    claims={claims}
                    showClaim={false}
                  />
                ) : c.judgement ? (
                  <div className="assessment">
                    <div className="assessment-head">
                      {c.judgement.shape ? <ShapeBadge shape={c.judgement.shape} /> : null}
                      <StandingBadge standing={c.judgement.standing} />
                      <span className="muted small">
                        {c.judgement.judgements} judgement{c.judgement.judgements === 1 ? '' : 's'}
                      </span>
                    </div>
                    <WitnessList witnesses={c.judgement.witnesses} />
                  </div>
                ) : (
                  <p className="muted small">Not judged yet.</p>
                )}
                {c.assessment && c.judgement ? (
                  <p className="small">
                    <StandingBadge standing={c.judgement.standing} />{' '}
                    <span className="muted">
                      {c.judgement.judgements} judgement{c.judgement.judgements === 1 ? '' : 's'}
                    </span>
                  </p>
                ) : null}
              </section>
            </div>
          </li>
        ))}
      </ol>
    </div>
  );
}

export function ConjectureList({
  conjectures,
  claims,
  scope,
}: {
  conjectures: readonly ConjectureFollowUp[];
  claims: readonly StatementLike[];
  scope?: string;
}) {
  if (conjectures.length === 0) return <p className="muted">No conjectures followed up.</p>;
  return (
    <ul className="conjecture-list">
      {conjectures.map((c) => (
        <li key={c.claim}>
          <div className="claim-head">
            <ClaimRef id={c.claim} claims={claims} scope={scope} />
            <ConjectureStateBadge state={c.state} />
            <span className="muted small">updated {formatDate(c.updated_at)}</span>
          </div>
          {c.state.state === 'not_pursued' ? <p className="small">{c.state.reason}</p> : null}
          {c.state.state === 'settled' ? (
            <div>
              <p className="small">
                <strong>{SETTLEMENT_LABELS[c.state.outcome]}.</strong>
              </p>
              <Markdown text={c.state.summary} />
            </div>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
