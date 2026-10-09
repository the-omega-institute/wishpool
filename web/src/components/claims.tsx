import type { ReactNode } from 'react';
import type { ClaimKind, ClaimRole, EscapeAssessment, FormalArtifact, Settles } from '../api/types';
import { isStandardAxiom, nonStandardAxioms } from '../lib/axioms';
import { CLAIM_KIND_LABELS } from '../lib/labels';
import { artifactLabel, commitHref, safeHttpUrl } from '../lib/links';
import { escapeRateTrend, formatEscapeRate } from '../lib/ratio';
import { KindBadge, LeanBadge, RoleBadge, ShapeBadge } from './badges';
import { LatexText, Markdown } from './Markdown';
import { Badge, ExternalLink, SourceLink } from './ui';

/** The fields every statement view needs; `Claim` and `PublicClaim` both have them. */
export interface StatementLike {
  id: string;
  kind: ClaimKind;
  role: ClaimRole;
  label: string;
  statement: string;
  section?: string;
  depends_on?: string[];
  settles?: Settles;
  has_proof?: boolean;
  lean?: FormalArtifact;
}

export function claimAnchor(id: string, scope = 'statement'): string {
  return `${scope}-${id.replace(/[^A-Za-z0-9_-]/g, '_')}`;
}

/** `Theorem 2.1`, or the kind and id when the paper gave no number. */
export function statementTitle(s: Pick<StatementLike, 'kind' | 'label' | 'id'>): string {
  const label = s.label.trim();
  return label !== '' ? label : `${CLAIM_KIND_LABELS[s.kind]} ${s.id}`;
}

export function ClaimRef({
  id,
  claims,
  scope,
}: {
  id: string;
  claims: readonly Pick<StatementLike, 'id' | 'kind' | 'label'>[];
  scope?: string;
}) {
  const claim = claims.find((c) => c.id === id);
  if (!claim) return <code title="Not among the confirmed statements">{id}</code>;
  return (
    <a href={`#${claimAnchor(id, scope)}`} className="claim-ref">
      {statementTitle(claim)}
    </a>
  );
}

export function StatementView({
  claim,
  claims,
  scope,
  children,
}: {
  claim: StatementLike;
  claims: readonly StatementLike[];
  scope?: string;
  children?: ReactNode;
}) {
  const deps = claim.depends_on ?? [];
  return (
    <div className="claim" id={claimAnchor(claim.id, scope)}>
      <div className="claim-head">
        <span className="claim-label">{statementTitle(claim)}</span>
        <code className="claim-id">{claim.id}</code>
        <KindBadge kind={claim.kind} />
        <RoleBadge role={claim.role} />
        {claim.has_proof === false && claim.kind !== 'conjecture' && claim.kind !== 'question' ? (
          <Badge tone="muted" title="No proof environment follows this statement in the source">
            No proof in source
          </Badge>
        ) : null}
        {claim.lean ? <LeanBadge artifact={claim.lean} /> : null}
      </div>
      {claim.section ? <p className="muted small claim-section">§ {claim.section}</p> : null}
      <LatexText source={claim.statement} className="statement" />
      {deps.length > 0 ? (
        <p className="claim-deps">
          Uses{' '}
          {deps.map((d, i) => (
            <span key={d}>
              {i > 0 ? ', ' : null}
              <ClaimRef id={d} claims={claims} scope={scope} />
            </span>
          ))}
        </p>
      ) : null}
      {claim.settles ? (
        <p className="claim-settles">
          Settles <em>{claim.settles.name}</em> (<SourceLink source={claim.settles.source} />)
        </p>
      ) : null}
      {children}
    </div>
  );
}

export function StatementList({
  claims,
  scope,
  empty = 'No statements.',
}: {
  claims: readonly StatementLike[];
  scope?: string;
  empty?: string;
}) {
  if (claims.length === 0) return <p className="muted">{empty}</p>;
  return (
    <ol className="claim-list">
      {claims.map((claim) => (
        <li key={claim.id}>
          <StatementView claim={claim} claims={claims} scope={scope} />
        </li>
      ))}
    </ol>
  );
}

function ArtifactLink({ value }: { value: string }) {
  const href = safeHttpUrl(value);
  return href ? <ExternalLink href={href}>{value}</ExternalLink> : <code>{value}</code>;
}

export function WitnessList({ witnesses }: { witnesses: readonly string[] }) {
  if (witnesses.length === 0) return null;
  return (
    <div className="witnesses">
      <h5>Escape witnesses</h5>
      <ol>
        {witnesses.map((w, i) => (
          <li key={i}>
            <Markdown text={w} />
          </li>
        ))}
      </ol>
    </div>
  );
}

export function EscapeAssessmentView({
  assessment,
  claims,
  scope,
  showClaim = true,
}: {
  assessment: EscapeAssessment;
  claims: readonly Pick<StatementLike, 'id' | 'kind' | 'label'>[];
  scope?: string;
  showClaim?: boolean;
}) {
  const rate = assessment.escape_rate;
  return (
    <div className="assessment">
      <div className="assessment-head">
        {showClaim ? <ClaimRef id={assessment.claim} claims={claims} scope={scope} /> : null}
        <ShapeBadge shape={assessment.shape} />
      </div>
      <WitnessList witnesses={assessment.witnesses} />
      {assessment.shape === 'content' && assessment.witnesses.length === 0 ? (
        <p className="muted">No witnesses named.</p>
      ) : null}
      {assessment.rationale ? <Markdown text={assessment.rationale} className="rationale" /> : null}
      {rate ? (
        <p className="escape-rate">
          <span className="escape-rate-label">Escape rate</span> on <em>{rate.arena}</em>:{' '}
          <span
            className="fraction"
            aria-label={`${formatEscapeRate(rate)}, ${escapeRateTrend(rate)}`}
          >
            {formatEscapeRate(rate)}
          </span>{' '}
          <span className="muted">
            (artifact <ArtifactLink value={rate.artifact} />)
          </span>
        </p>
      ) : null}
    </div>
  );
}

/** A verified Lean proof: repository at commit, declarations, axioms. */
export function FormalArtifactView({
  artifact,
  axioms,
}: {
  artifact: FormalArtifact;
  axioms?: readonly string[];
}) {
  const href = commitHref(artifact);
  const flagged = axioms ? nonStandardAxioms(axioms) : [];
  return (
    <div className="formal">
      <p className="formal-artifact">
        {href ? (
          <ExternalLink href={href}>
            <code>{artifactLabel(artifact)}</code>
          </ExternalLink>
        ) : (
          <code>{artifactLabel(artifact)}</code>
        )}
        {flagged.length > 0 ? (
          <>
            {' '}
            <Badge tone="bad" title="Uses axioms beyond propext, Classical.choice and Quot.sound">
              Non-standard axioms
            </Badge>
          </>
        ) : null}
      </p>
      {artifact.declarations.length > 0 ? (
        <p className="formal-decls">
          Declarations:{' '}
          {artifact.declarations.map((d, i) => (
            <span key={d}>
              {i > 0 ? ', ' : null}
              <code>{d}</code>
            </span>
          ))}
        </p>
      ) : null}
      {axioms ? (
        <p className="axioms">
          Axioms:{' '}
          {axioms.length === 0 ? (
            <span className="muted">none</span>
          ) : (
            axioms.map((a, i) => (
              <span key={a}>
                {i > 0 ? ', ' : null}
                <code className={isStandardAxiom(a) ? undefined : 'axiom-flag'}>{a}</code>
              </span>
            ))
          )}
        </p>
      ) : null}
    </div>
  );
}
