import type {
  AdmissionBasis,
  AiUseLevel,
  AnalysisVisibility,
  Author,
  ClaimKind,
  ConjectureState,
  Decision,
  ItemState,
  JudgementStanding,
  Outcome,
  PriorRelation,
  ProofShape,
  RejectReason,
  Reviewer,
  Role,
  SettlementOutcome,
  SourceKind,
  Stage,
  SubmissionStatus,
  Recommendation,
  Severity,
  ImprovementKind,
  Effort,
  ImprovementEvidence,
  Feasibility,
  StepState,
  ProbeOutcome,
} from '../api/types';
import { sourceLabel } from './links';

export const RECOMMENDATION_LABELS: { readonly [K in Recommendation]: string } = {
  accept: 'accept',
  minor_revision: 'minor revision',
  major_revision: 'major revision',
  reject: 'reject',
};
export const SEVERITY_LABELS: { readonly [K in Severity]: string } = {
  major: 'Major concerns',
  minor: 'Minor concerns',
};
export const IMPROVEMENT_KIND_LABELS: { readonly [K in ImprovementKind]: string } = {
  gap: 'Gap',
  strengthen: 'Strengthen',
  generalize: 'Generalize',
  computation: 'Computation',
  literature: 'Literature',
  exposition: 'Exposition',
};
export const EFFORT_LABELS: { readonly [K in Effort]: string } = {
  small: 'Small',
  medium: 'Medium',
  large: 'Large',
};
export const EVIDENCE_LABELS: { readonly [K in ImprovementEvidence]: string } = {
  checked: 'Checked',
  proposed: 'Proposed',
};
export const FEASIBILITY_LABELS: { readonly [K in Feasibility]: string } = {
  ready: 'Ready',
  needs_library: 'Needs library',
  hard: 'Hard',
};
export const PROBE_OUTCOME_LABELS: { readonly [K in ProbeOutcome]: string } = {
  compiled: 'Compiled in Lean',
  failed: 'Not compiled',
};
export const STEP_STATE_LABELS: { readonly [K in StepState<unknown>['state']]: string } = {
  pending: 'Pending',
  running: 'Running',
  done: 'Done',
  failed: 'Failed',
  skipped: 'Skipped',
};

export const SUBMISSION_STATE_LABELS: { readonly [K in SubmissionStatus['state']]: string } = {
  draft: 'Draft — statements to confirm',
  in_review: 'In review',
  accepted: 'Accepted',
  not_accepted: 'Not accepted',
  withdrawn: 'Withdrawn',
};

export const AI_USE_LABELS: { readonly [K in AiUseLevel]: string } = {
  none: 'None',
  assisted: 'Assisted',
  substantial: 'Substantial',
  primarily: 'Primarily AI',
};
export const AI_USE_DESCRIPTIONS: { readonly [K in AiUseLevel]: string } = {
  none: 'No AI system contributed to the mathematics or the text.',
  assisted: 'AI tools assisted with search, editing or routine steps.',
  substantial: 'AI systems produced substantial parts of the proofs or text.',
  primarily: 'The mathematics was produced primarily by AI systems.',
};
export const AI_USES = Object.keys(AI_USE_LABELS) as AiUseLevel[];

export const CLAIM_KIND_LABELS: { readonly [K in ClaimKind]: string } = {
  theorem: 'Theorem',
  proposition: 'Proposition',
  lemma: 'Lemma',
  corollary: 'Corollary',
  claim: 'Claim',
  conjecture: 'Conjecture',
  question: 'Question',
};
export const CLAIM_KINDS = Object.keys(CLAIM_KIND_LABELS) as ClaimKind[];

/** Conjectures and questions are open statements: the paper does not prove them. */
export function isOpenKind(kind: ClaimKind): boolean {
  return kind === 'conjecture' || kind === 'question';
}

export const BASIS_LABELS: { readonly [K in AdmissionBasis]: string } = {
  escape_witness: 'Escape witness',
  open_problem_settlement: 'Open-problem settlement',
};
export const BASIS_DESCRIPTIONS: { readonly [K in AdmissionBasis]: string } = {
  escape_witness:
    'A main result carries an escape witness (judged content) and no prior work states or directly implies it.',
  open_problem_settlement:
    'A main result settles a named, sourced open problem that the literature had not settled.',
};

export const ROLES: readonly Role[] = ['editor', 'reviewer', 'endorser', 'admin'];
export const ROLE_LABELS: { readonly [K in Role]: string } = {
  editor: 'Editor',
  reviewer: 'Reviewer',
  endorser: 'Endorser',
  admin: 'Admin',
};

export const SOURCE_KIND_LABELS: { readonly [K in SourceKind]: string } = {
  arxiv: 'arXiv',
  doi: 'DOI',
  zenodo: 'Zenodo',
  hexagon: 'Hexagon',
  oeis: 'OEIS',
  url: 'URL',
  personal: 'Personal communication',
};
export const SOURCE_KINDS = Object.keys(SOURCE_KIND_LABELS) as SourceKind[];

export const SHAPE_LABELS: { readonly [K in ProofShape]: string } = {
  bind_only: 'Bind-only',
  content: 'Content',
};
export const SHAPE_DESCRIPTIONS: { readonly [K in ProofShape]: string } = {
  bind_only:
    'The proof instantiates, projects or normalises prior results; no new proposition appears on its proof path.',
  content:
    'The proof path contains escape witnesses: new propositions that prior results do not give by binding alone.',
};

export const RELATION_LABELS: { readonly [K in PriorRelation]: string } = {
  same: 'States the result',
  implies: 'Directly implies it',
  related: 'Related',
};
export const RELATIONS = Object.keys(RELATION_LABELS) as PriorRelation[];

export const STANDING_LABELS: { readonly [K in JudgementStanding]: string } = {
  confirmed: 'Confirmed by an editor',
  corroborated: 'Corroborated',
  proposed: 'Proposed',
  disputed: 'Disputed',
};
export const STANDING_DESCRIPTIONS: { readonly [K in JudgementStanding]: string } = {
  confirmed: 'An editor’s judgement decides.',
  corroborated:
    'Two machine judgements from different model families and accounts agree; an editor has not yet decided.',
  proposed: 'A single judgement, not yet corroborated or confirmed.',
  disputed: 'Judgements disagree; an editor decides.',
};

export const VISIBILITY_LABELS: { readonly [K in AnalysisVisibility]: string } = {
  undecided: 'Not chosen yet (private)',
  public: 'Public',
  private: 'Private',
};

export const ITEM_STATE_LABELS: { readonly [K in ItemState['state']]: string } = {
  proposed: 'Proposed — awaiting the author',
  approved: 'Approved by the author',
  declined: 'Declined by the author',
  in_progress: 'In progress',
  verified: 'Lean verified',
};

export const CONJECTURE_STATE_LABELS: { readonly [K in ConjectureState['state']]: string } = {
  screening: 'Screening',
  taken_up: 'Taken up',
  not_pursued: 'Not pursued',
  settled: 'Settled',
};
export const CONJECTURE_STATES = Object.keys(CONJECTURE_STATE_LABELS) as ConjectureState['state'][];
export const SETTLEMENT_LABELS: { readonly [K in SettlementOutcome]: string } = {
  proved: 'Proved',
  disproved: 'Disproved',
  partial: 'Partially resolved',
};
export const SETTLEMENT_OUTCOMES = Object.keys(SETTLEMENT_LABELS) as SettlementOutcome[];

export function conjectureStateText(state: ConjectureState): string {
  switch (state.state) {
    case 'screening':
    case 'taken_up':
      return CONJECTURE_STATE_LABELS[state.state];
    case 'not_pursued':
      return `Not pursued: ${state.reason}`;
    case 'settled':
      return `Settled — ${SETTLEMENT_LABELS[state.outcome].toLowerCase()}`;
  }
}

/** A reason a paper was not accepted, as a sentence the author can act on. */
export function rejectReasonText(reason: RejectReason): string {
  switch (reason.reason) {
    case 'hygiene':
      return `Source: ${reason.detail}`;
    case 'known_result':
      return `Known result: statement ${reason.claim} is stated in, or directly implied by, ${sourceLabel(reason.prior)}.`;
    case 'bind_only':
      return 'Bind-only: no main result carries new content; each follows from prior results by instantiation, projection or normalisation.';
    case 'no_main_result':
      return 'No main result: the paper marks no proved statement as a main result.';
    case 'out_of_scope':
      return `Out of scope: ${reason.detail}`;
  }
}

export function outcomeLabel(outcome: Outcome): string {
  switch (outcome.outcome) {
    case 'pass':
      return 'Pass';
    case 'fail':
      return 'Fail';
    case 'needs_human':
      return 'Needs an editor';
  }
}

export function reviewerLabel(reviewer: Reviewer): string {
  if (reviewer.kind === 'human') return 'Editor';
  return reviewer.model
    ? `Machine · ${reviewer.engine} (${reviewer.model})`
    : `Machine · ${reviewer.engine}`;
}

export function decisionHeadline(decision: Decision, nameOf: (stage: Stage) => string): string {
  switch (decision.decision) {
    case 'pending':
      return `Pending — awaiting ${nameOf(decision.awaiting)}`;
    case 'accept':
      return `Accept — ${BASIS_LABELS[decision.basis]}`;
    case 'not_accepted':
      return 'Not accepted';
  }
}

/** `A. One, B. Two and C. Three`. */
export function formatAuthors(authors: readonly Author[]): string {
  const names = authors.map((a) => a.name.trim()).filter((n) => n !== '');
  if (names.length <= 1) return names[0] ?? '';
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

/** `11B05, 05D10` → `['11B05', '05D10']`, deduplicated, upper-cased. */
export function parseMsc(text: string): string[] {
  const seen = new Set<string>();
  for (const raw of text.split(/[\s,;]+/)) {
    const code = raw.trim().toUpperCase();
    if (code !== '') seen.add(code);
  }
  return [...seen];
}

/** A short, stable form of an opaque identifier for display. */
export function shortId(id: string): string {
  return id.length > 14 ? `${id.slice(0, 6)}…${id.slice(-4)}` : id;
}

/** `1.2 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
