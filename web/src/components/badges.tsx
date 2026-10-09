import type {
  AdmissionBasis,
  ClaimKind,
  ClaimRole,
  ConjectureState,
  FormalArtifact,
  ItemState,
  JudgementStanding,
  Outcome,
  ProofShape,
  SubmissionStatus,
} from '../api/types';
import {
  BASIS_DESCRIPTIONS,
  BASIS_LABELS,
  CLAIM_KIND_LABELS,
  CONJECTURE_STATE_LABELS,
  ITEM_STATE_LABELS,
  SHAPE_DESCRIPTIONS,
  SHAPE_LABELS,
  STANDING_DESCRIPTIONS,
  STANDING_LABELS,
  SUBMISSION_STATE_LABELS,
  outcomeLabel,
} from '../lib/labels';
import { artifactLabel, commitHref } from '../lib/links';
import { Badge, ExternalLink, type BadgeTone } from './ui';

const SUBMISSION_TONES: { [K in SubmissionStatus['state']]: BadgeTone } = {
  draft: 'warn',
  in_review: 'accent',
  accepted: 'good',
  not_accepted: 'bad',
  withdrawn: 'muted',
};

export function SubmissionStatusBadge({ status }: { status: SubmissionStatus }) {
  return (
    <Badge tone={SUBMISSION_TONES[status.state]}>{SUBMISSION_STATE_LABELS[status.state]}</Badge>
  );
}

export function BasisBadge({ basis }: { basis: AdmissionBasis }) {
  return (
    <Badge tone="neutral" title={BASIS_DESCRIPTIONS[basis]}>
      {BASIS_LABELS[basis]}
    </Badge>
  );
}

export function KindBadge({ kind }: { kind: ClaimKind }) {
  return <Badge tone="neutral">{CLAIM_KIND_LABELS[kind]}</Badge>;
}

export function RoleBadge({ role }: { role: ClaimRole }) {
  return (
    <Badge tone={role === 'main' ? 'accent' : 'muted'}>
      {role === 'main' ? 'Main result' : 'Supporting'}
    </Badge>
  );
}

export function ShapeBadge({ shape }: { shape: ProofShape }) {
  return (
    <Badge tone={shape === 'content' ? 'good' : 'muted'} title={SHAPE_DESCRIPTIONS[shape]}>
      <span className="visually-hidden">Shape: </span>
      {SHAPE_LABELS[shape]}
    </Badge>
  );
}

const STANDING_TONES: { [K in JudgementStanding]: BadgeTone } = {
  confirmed: 'good',
  corroborated: 'accent',
  proposed: 'muted',
  disputed: 'warn',
};

export function StandingBadge({ standing }: { standing: JudgementStanding }) {
  return (
    <Badge tone={STANDING_TONES[standing]} title={STANDING_DESCRIPTIONS[standing]}>
      {STANDING_LABELS[standing]}
    </Badge>
  );
}

const OUTCOME_TONES: { [K in Outcome['outcome']]: BadgeTone } = {
  pass: 'good',
  fail: 'bad',
  needs_human: 'warn',
};

export function OutcomeBadge({ outcome, proposal }: { outcome: Outcome; proposal?: boolean }) {
  if (proposal) {
    return (
      <Badge tone="warn" title="A machine pass on a judgement stage awaits an editor’s report.">
        Pass (proposal)
      </Badge>
    );
  }
  return <Badge tone={OUTCOME_TONES[outcome.outcome]}>{outcomeLabel(outcome)}</Badge>;
}

const ITEM_TONES: { [K in ItemState['state']]: BadgeTone } = {
  proposed: 'warn',
  approved: 'accent',
  declined: 'muted',
  in_progress: 'accent',
  verified: 'good',
};

export function ItemStateBadge({ state }: { state: ItemState }) {
  return <Badge tone={ITEM_TONES[state.state]}>{ITEM_STATE_LABELS[state.state]}</Badge>;
}

const CONJECTURE_TONES: { [K in ConjectureState['state']]: BadgeTone } = {
  screening: 'muted',
  taken_up: 'accent',
  not_pursued: 'muted',
  settled: 'good',
};

export function ConjectureStateBadge({ state }: { state: ConjectureState }) {
  return <Badge tone={CONJECTURE_TONES[state.state]}>{CONJECTURE_STATE_LABELS[state.state]}</Badge>;
}

/** "Lean verified", linked to the repository at the pinned commit. */
export function LeanBadge({ artifact }: { artifact: FormalArtifact }) {
  const href = commitHref(artifact);
  const label = artifactLabel(artifact);
  const badge = (
    <Badge tone="good" title={`Proved in Lean: ${label}`}>
      Lean verified
    </Badge>
  );
  return (
    <span className="lean-badge">
      {badge}{' '}
      {href ? (
        <ExternalLink href={href}>
          <code>{label}</code>
        </ExternalLink>
      ) : (
        <code>{label}</code>
      )}
    </span>
  );
}
