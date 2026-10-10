import { useState } from 'react';
import type {
  AuditedClaim,
  Claim,
  Correctness,
  FormalArtifact,
  RefereeRound,
  Submission,
} from '../api/types';
import { CLAIM_KIND_LABELS, isOpenKind } from '../lib/labels';
import { LeanBadge } from './badges';
import { LatexText } from './Markdown';
import { claimAnchor, type StatementLike } from './claims';

const VERDICTS: Record<Correctness, { label: string; tone: string }> = {
  correct: { label: 'Correct', tone: 'good' },
  gap: { label: 'Gap', tone: 'warn' },
  error: { label: 'Error', tone: 'bad' },
  not_checked: { label: 'Not checked', tone: 'muted' },
};

/** The statement's own title from `Theorem (Title) [key]`, without the LaTeX key. */
export function statementName(claim: Pick<Claim, 'label'>): string | null {
  const match = /\((.+)\)/.exec(claim.label);
  const title = match?.[1]?.trim();
  return title ? title : null;
}

function novelty(
  claim: Pick<Claim, 'role' | 'kind'>,
  reading: AuditedClaim | undefined,
): string | null {
  if (claim.role !== 'main' || isOpenKind(claim.kind) || !reading) return null;
  if (reading.known) return 'Already known';
  if (reading.correctness !== 'correct') return null;
  if (reading.shape === 'content') return 'New';
  if (reading.shape === 'bind_only') return 'Follows from known results';
  return null;
}

export function StatementSummary({
  submission,
  round,
}: {
  submission: Submission;
  round?: RefereeRound;
}) {
  const audit = round?.audit?.state.state === 'done' ? round.audit.state.result : undefined;
  const probe = round?.formal?.state.state === 'done' ? round.formal.state.result : undefined;
  const main = submission.claims.filter((c) => c.role === 'main');
  const supporting = submission.claims.filter((c) => c.role !== 'main');
  const row = (claim: Claim) => {
    const attempt = probe?.attempts.find((a) => a.claim === claim.id);
    const proved =
      attempt?.outcome === 'compiled' ||
      submission.formalization.items.some(
        (i) => i.claim === claim.id && i.state.state === 'verified',
      );
    return (
      <StatementRow
        key={claim.id}
        claim={claim}
        reading={audit?.claims.find((c) => c.claim === claim.id)}
        reviewed={audit !== undefined}
        proved={proved}
        theorem={attempt?.outcome === 'compiled' ? attempt.theorem : undefined}
      />
    );
  };
  return (
    <section className="statements" aria-labelledby="statement-summary-h">
      <h2 id="statement-summary-h">Statements</h2>
      {main.length > 0 ? (
        <>
          <h3 className="statements-group">
            {main.length === 1 ? 'Main result' : `Main results · ${main.length}`}
          </h3>
          <ul className="statement-list">{main.map(row)}</ul>
        </>
      ) : null}
      {supporting.length > 0 ? (
        <>
          <h3 className="statements-group">Supporting · {supporting.length}</h3>
          <ul className="statement-list">{supporting.map(row)}</ul>
        </>
      ) : null}
    </section>
  );
}

/** The public statement list: the same rows as the author's, without any review labels. */
export function PublicStatements({ claims }: { claims: readonly StatementLike[] }) {
  const main = claims.filter((c) => c.role === 'main');
  const supporting = claims.filter((c) => c.role !== 'main');
  const row = (claim: StatementLike) => (
    <StatementRow
      key={claim.id}
      claim={claim}
      reviewed={false}
      proved={Boolean(claim.lean)}
      uses={claim.depends_on ?? []}
      artifact={claim.lean ?? undefined}
      anchor={claimAnchor(claim.id, 'public')}
    />
  );
  return (
    <>
      {main.length > 0 ? (
        <>
          <h3 className="statements-group">
            {main.length === 1 ? 'Main result' : `Main results · ${main.length}`}
          </h3>
          <ul className="statement-list">{main.map(row)}</ul>
        </>
      ) : null}
      {supporting.length > 0 ? (
        <>
          <h3 className="statements-group">Supporting · {supporting.length}</h3>
          <ul className="statement-list">{supporting.map(row)}</ul>
        </>
      ) : null}
    </>
  );
}

function StatementRow({
  claim,
  reading,
  reviewed,
  proved,
  theorem,
  uses,
  anchor,
  artifact,
}: {
  claim: Pick<Claim, 'id' | 'kind' | 'role' | 'label' | 'statement'>;
  reading?: AuditedClaim;
  reviewed: boolean;
  proved: boolean;
  theorem?: string;
  uses?: readonly string[];
  anchor?: string;
  artifact?: FormalArtifact;
}) {
  const [open, setOpen] = useState(false);
  const name = statementName(claim);
  const verdict = reviewed ? VERDICTS[reading?.correctness ?? 'not_checked'] : null;
  const isNew = novelty(claim, reading);
  const flagged = reading && (reading.correctness === 'gap' || reading.correctness === 'error');
  const bodyId = `stmt-${claim.id}`;
  return (
    <li id={anchor} className={open ? 'statement-item is-open' : 'statement-item'}>
      <button
        type="button"
        className="statement-row"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen(!open)}
      >
        <span className="statement-kind">
          {CLAIM_KIND_LABELS[claim.kind]} <span className="statement-id">{claim.id}</span>
        </span>
        <span className="statement-name">
          <LatexText source={name ?? claim.statement} className="statement-preview" />
        </span>
        <span className="statement-tags">
          {verdict ? <span className={`tag tag-${verdict.tone}`}>{verdict.label}</span> : null}
          {isNew ? (
            <span className={isNew === 'New' ? 'tag tag-accent' : 'tag tag-muted'}>{isNew}</span>
          ) : null}
          {proved ? <span className="tag tag-good">Lean ✓</span> : null}
        </span>
      </button>
      {flagged && !open ? <p className="statement-note">{reading.comment}</p> : null}
      <div id={bodyId} className="statement-body" hidden={!open}>
        <LatexText source={claim.statement} className="statement" />
        {reading?.comment ? <p className="statement-note">{reading.comment}</p> : null}
        {artifact ? (
          <p className="small">
            <LeanBadge artifact={artifact} />
          </p>
        ) : null}
        {uses && uses.length > 0 ? <p className="small muted">Uses {uses.join(', ')}.</p> : null}
        {theorem ? (
          <p className="small muted">
            Proved in Lean 4 as <code>{theorem}</code>.
          </p>
        ) : null}
      </div>
    </li>
  );
}
