/*
 * Wire types for the wishpool HTTP API v3, copied from `docs/API.md`.
 * Keep this file in step with that document; the frontend calls no route
 * the document does not list.
 */

// ── Errors and listings ──────────────────────────────────────────────────

export type ProblemCode =
  | 'not_authenticated'
  | 'forbidden'
  | 'not_found'
  | 'invalid'
  | 'conflict'
  | 'stale_revision'
  | 'unavailable';

/** An `application/problem+json` body, read defensively. */
export interface Problem {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  code?: string;
}

export type Listing<T> = { items: T[]; next_before: string | null };

// ── Referee rounds and editorial feedback ────────────────────────────────

export type Recommendation = 'accept' | 'minor_revision' | 'major_revision' | 'reject';
export type Severity = 'major' | 'minor';
export type ImprovementKind =
  'gap' | 'strengthen' | 'generalize' | 'computation' | 'literature' | 'exposition';
export type Effort = 'small' | 'medium' | 'large';
export type ImprovementEvidence = 'checked' | 'proposed';
export type Feasibility = 'ready' | 'needs_library' | 'hard';

export type RefereeConcern = { claim?: string; severity: Severity; issue: string };
export type RefereeClaim = {
  claim: string;
  shape: 'content' | 'bind_only';
  witnesses: string[];
  known?: string;
  note: string;
};
export type RefereeReport = {
  recommendation?: Recommendation;
  summary: string;
  strengths: string[];
  concerns: RefereeConcern[];
  claims: RefereeClaim[];
  limits: string[];
  text: string;
};
export type Improvement = {
  claim?: string;
  kind: ImprovementKind;
  suggestion: string;
  how_we_help: string;
  effort: Effort;
  status: ImprovementEvidence;
  evidence: string;
};
export type FormalizationCandidate = {
  claim: string;
  feasibility: Feasibility;
  mathlib: string[];
  missing: string[];
  lean_sketch: string;
  plan: string;
  effort: Effort;
};
export type Advice = {
  summary: string;
  improvements: Improvement[];
  formalization: FormalizationCandidate[];
};
export type LetterDraft = { subject: string; body: string; note: string };
export type ProbeOutcome = 'compiled' | 'failed';
export type FormalAttempt = {
  claim: string;
  outcome: ProbeOutcome;
  theorem?: string;
  lean: string;
  axioms: string[];
  note: string;
  log: string;
};
/** A private, staff-only formalization probe; never a recorded formalization. */
export type FormalProbe = { toolchain: string; attempts: FormalAttempt[]; summary: string };

export type StepState<T> =
  | { state: 'pending' }
  | { state: 'running'; task?: string; queue_position?: number; since: string }
  | { state: 'done'; result: T; at: string }
  | { state: 'failed'; reason: string; detail?: string; retryable: boolean; at: string }
  | { state: 'skipped'; reason: string };
export type Step<T> = {
  engine?: string;
  model?: string;
  attempts: number;
  client_ref?: string;
  input_digest?: string;
  state: StepState<T>;
};
export type RefereeRound = {
  number: number;
  version: number;
  claims_revision: number;
  started_at: string;
  referee: Step<RefereeReport>;
  advice: Step<Advice>;
  /** Absent on rounds stored before the probe existed. */
  formal?: Step<FormalProbe>;
  letter: Step<LetterDraft>;
};
export type FeedbackLetter = {
  round?: number;
  subject: string;
  body: string;
  note: string;
  edited: boolean;
  sent_by: string;
  sent_at: string;
};
export type RefereeFile = {
  id: string;
  rounds: RefereeRound[];
  letters: FeedbackLetter[];
  revision: number;
};
export type SendFeedback = { subject: string; body: string; note?: string };

// ── People and sign-in ───────────────────────────────────────────────────

export type Role = 'editor' | 'endorser' | 'reviewer' | 'admin';
export type Person = {
  id: string;
  display_name: string;
  email?: string;
  picture?: string;
  orcid?: string;
  affiliation?: string;
  roles: Role[];
  created_at: string;
  last_seen_at: string;
};

/** `GET /auth/session`. */
export type Session =
  | { authenticated: false; donations_enabled?: boolean; dev_sign_in?: boolean }
  | { authenticated: true; person: Person; donations_enabled?: boolean; dev_sign_in?: boolean };

// ── Policy ───────────────────────────────────────────────────────────────

export type Stage = 'hygiene' | 'claims' | 'literature' | 'escape';
/** Stages an editor or reviewer account files reports on (`claims` is the author's). */
export type ReportedStage = Exclude<Stage, 'claims'>;

export type Policy = {
  human_judgement: Stage[];
  required_endorsements: number;
  max_active_per_author: number;
};
export type PolicyStage = { stage: Stage; code: string; title: string; description: string };
/** `GET /policy`. */
export type PolicyDocument = { policy: Policy; stages: PolicyStage[] };

// ── Papers ───────────────────────────────────────────────────────────────

export type AiUseLevel = 'none' | 'assisted' | 'substantial' | 'primarily';
export type AiDisclosure = { level: AiUseLevel; statement: string };

export type NewPaper = {
  ai_disclosure: AiDisclosure;
  /** Overrides `\author`. */
  authors?: Author[];
  /** e.g. "11B83". */
  msc?: string[];
  /** The paper's persistent identifier, when it already has one. */
  doi?: string | null;
  open_to_contributors?: boolean;
};
export type Author = { name: string; person?: string; orcid?: string; affiliation?: string };

export type SourceKind = 'doi' | 'arxiv' | 'hexagon' | 'zenodo' | 'oeis' | 'url' | 'personal';
export type Source = { kind: SourceKind; locator: string; year?: number };

export type ClaimKind =
  'theorem' | 'proposition' | 'lemma' | 'corollary' | 'claim' | 'conjecture' | 'question';
export type ClaimRole = 'main' | 'supporting';
export type Settles = { name: string; source: Source };
export type Claim = {
  /** C1, C2, … */
  id: string;
  kind: ClaimKind;
  label: string;
  latex_label?: string;
  /** LaTeX. */
  statement: string;
  role: ClaimRole;
  has_proof: boolean;
  section?: string;
  depends_on: string[];
  settles?: Settles;
};
export type ClaimConfirmation = {
  id: string;
  kind: ClaimKind;
  role: ClaimRole;
  depends_on?: string[];
  settles?: Settles | null;
  excluded?: boolean;
};

export type Blob = { id: string; bytes: number; sha256: string };
export type PaperVersion = {
  number: number;
  archive: Blob;
  filename: string;
  main_file: string;
  pdf?: Blob;
  compile_error?: string;
  parse_warnings: string[];
  /** Preamble math macros, KaTeX syntax (`"\\rep": "\\operatorname{rep}"`). */
  macros: Record<string, string>;
  note: string;
  uploaded_at: string;
};

export type SubmissionStatus =
  | { state: 'draft' }
  | { state: 'in_review' }
  | { state: 'accepted'; record: string }
  | { state: 'not_accepted' }
  | { state: 'withdrawn' };
export type AnalysisVisibility = 'undecided' | 'public' | 'private';

export type Submission = {
  id: string;
  submitter: string;
  title: string;
  abstract_text: string;
  authors: Author[];
  ai_disclosure: AiDisclosure;
  msc: string[];
  doi?: string;
  /** The last is current. */
  versions: PaperVersion[];
  /** Read from the current version, awaiting confirmation. */
  extracted: Claim[];
  /** Confirmed. */
  claims: Claim[];
  claims_revision: number;
  /** Append-only; the latest per stage for the current claims counts. */
  reports: StageReport[];
  status: SubmissionStatus;
  decision?: Decision;
  open_to_contributors: boolean;
  analysis_visibility: AnalysisVisibility;
  formalization: { repository?: string; items: FormalizationItem[] };
  conjectures: ConjectureFollowUp[];
  created_at: string;
  updated_at: string;
  revision: number;
};

// ── Review ───────────────────────────────────────────────────────────────

export type Evidence = { kind: string; locator: string; note: string };
export type Ratio = { numerator: number; denominator: number };

export type RejectReason =
  | { reason: 'hygiene'; detail: string }
  | { reason: 'known_result'; claim: string; prior: Source }
  | { reason: 'bind_only' }
  | { reason: 'no_main_result' }
  | { reason: 'out_of_scope'; detail: string };

export type Outcome =
  | { outcome: 'pass' }
  | { outcome: 'fail'; reason: RejectReason }
  | { outcome: 'needs_human'; question: string };

export type Reviewer =
  | { kind: 'human'; person: string }
  | { kind: 'machine'; account: string; engine: string; model?: string };

export type HygieneCheck = { name: string; passed: boolean; detail: string };
export type PriorRelation = 'same' | 'implies' | 'related';
export type PriorWork = { claim: string; source: Source; relation: PriorRelation; note: string };
export type ProofShape = 'bind_only' | 'content';
export type EscapeRate = { arena: string; before: Ratio; after: Ratio; artifact: string };
export type EscapeAssessment = {
  claim: string;
  shape: ProofShape;
  witnesses: string[];
  rationale: string;
  escape_rate?: EscapeRate;
};

export type StagePayload =
  | { stage: 'hygiene'; checks: HygieneCheck[] }
  | { stage: 'claims'; claims: Claim[] }
  | { stage: 'literature'; prior: PriorWork[]; searched: string[] }
  | { stage: 'escape'; assessments: EscapeAssessment[] };

export type StageReport = {
  stage: Stage;
  outcome: Outcome;
  summary: string;
  payload: StagePayload;
  evidence: Evidence[];
  reviewer: Reviewer;
  claims_revision: number;
  filed_at: string;
};
export type ReportDraft = {
  outcome: Outcome;
  summary: string;
  payload: StagePayload;
  evidence?: Evidence[];
};
/** `POST /submissions/{id}/stages/{stage}/reports`. */
export type FileReport = { report: ReportDraft; filed_by?: { engine: string; model?: string } };

export type AdmissionBasis = 'escape_witness' | 'open_problem_settlement';
export type Decision =
  | { decision: 'pending'; awaiting: Stage; detail: string }
  | { decision: 'accept'; basis: AdmissionBasis }
  | { decision: 'not_accepted'; reasons: RejectReason[] };

export type NewJudgement = { shape: ProofShape; witnesses: string[]; rationale: string };
export type ClaimJudgement = {
  id: string;
  submission: string;
  claim: string;
  claims_revision: number;
  shape: ProofShape;
  witnesses: string[];
  rationale: string;
  reviewer: Reviewer;
  task?: string;
  filed_at: string;
};

export type JudgementStanding = 'confirmed' | 'corroborated' | 'proposed' | 'disputed';
export type ClaimAnalysis = {
  claim: string;
  kind: ClaimKind;
  role: ClaimRole;
  label: string;
  prior: PriorWork[];
  known: boolean;
  assessment?: EscapeAssessment;
  judgement?: {
    shape?: ProofShape;
    standing: JudgementStanding;
    witnesses: string[];
    judgements: number;
  };
  formalization?: ItemState;
};
export type PaperAnalysis = {
  submission: string;
  main_results: number;
  main_with_content: number;
  main_known: number;
  open_statements: number;
  formalized: number;
  claims: ClaimAnalysis[];
};

// ── After acceptance ─────────────────────────────────────────────────────

export type FormalArtifact = {
  repository: string;
  /** 40 hex. */
  commit: string;
  declarations: string[];
};
export type ItemState =
  | { state: 'proposed' }
  | { state: 'approved' }
  | { state: 'declined'; reason: string }
  | { state: 'in_progress' }
  | { state: 'verified'; artifact: FormalArtifact; axioms: string[]; contribution?: string };
export type FormalizationItem = {
  claim: string;
  reason: string;
  state: ItemState;
  updated_at: string;
};
export type FormalVerification = {
  artifact: FormalArtifact;
  axioms: string[];
  contribution?: string;
};

export type SettlementOutcome = 'proved' | 'disproved' | 'partial';
export type ConjectureState =
  | { state: 'screening' }
  | { state: 'taken_up' }
  | { state: 'not_pursued'; reason: string }
  | { state: 'settled'; outcome: SettlementOutcome; summary: string; evidence: Evidence[] };
export type ConjectureFollowUp = { claim: string; state: ConjectureState; updated_at: string };

// ── Public papers ────────────────────────────────────────────────────────

export type PaperSummary = {
  record: string;
  submission: string;
  title: string;
  authors: Author[];
  abstract_text: string;
  msc: string[];
  doi?: string;
  basis: AdmissionBasis;
  accepted_at: string;
  main_results: number;
  lean_verified: number;
};
export type PublicClaim = {
  id: string;
  kind: ClaimKind;
  role: ClaimRole;
  label: string;
  statement: string;
  section?: string;
  lean?: FormalArtifact;
};
export type PublicPaper = {
  summary: PaperSummary;
  ai_disclosure: AiDisclosure;
  versions: { number: number; uploaded_at: string; has_pdf: boolean; note: string }[];
  claims: PublicClaim[];
  formalization_repository?: string;
  /** Of the current version, for rendering statements. */
  macros: Record<string, string>;
  /** Only when the author made it public. */
  analysis?: PaperAnalysis;
  /** Only when the author made the analysis public. */
  conjectures: ConjectureFollowUp[];
};

// ── Contributors ─────────────────────────────────────────────────────────

export type TaskKind = 'judge_escape' | 'literature_check' | 'formalize' | 'probe';
export type ContributionMode = 'own_agent' | 'hosted';
export type TaskStatus =
  | { state: 'open' }
  | { state: 'leased'; lease: { holder: string; mode: ContributionMode; until: string } }
  | { state: 'submitted'; contribution: string }
  | { state: 'done'; contribution: string }
  | { state: 'closed'; reason: string };
export type TaskState = TaskStatus['state'];
export type Task = {
  id: string;
  kind: TaskKind;
  target: { submission: string; claim: string };
  dedupe_key: string;
  title: string;
  contributors: string[];
  status: TaskStatus;
  created_by: string;
  created_at: string;
  updated_at: string;
  revision: number;
};
export type TaskContext = {
  task: Task;
  paper_title: string;
  abstract_text: string;
  claim: Claim;
  dependencies: Claim[];
  repository?: string;
  pdf_public: boolean;
  nature: string;
  macros: Record<string, string>;
};
/** `POST /submissions/{id}/tasks`. */
export type TaskGeneration = { created: number; existing: number };

export type ContributionOutput =
  | { output: 'judgement'; shape: ProofShape; witnesses: string[]; rationale: string }
  | { output: 'literature'; prior: PriorWork[]; searched: string[]; summary: string }
  | { output: 'probe_note'; note: string }
  | { output: 'pull_request'; url: string };
export type NewContribution = {
  agent: { tool: string; model: string };
  output: ContributionOutput;
  tokens?: { input: number; output: number };
};
export type ContributionStatus =
  | { state: 'submitted' }
  | { state: 'verified'; detail: string }
  | { state: 'rejected'; reason: string };
export type Contribution = {
  id: string;
  task: string;
  kind: TaskKind;
  contributor: string;
  mode: ContributionMode;
  agent: { tool: string; model: string };
  output: ContributionOutput;
  tokens?: { input: number; output: number; metered: boolean };
  status: ContributionStatus;
  submitted_at: string;
  updated_at: string;
  revision: number;
};
/** `POST /tasks/{id}/contributions`. */
export type SubmittedContribution = { contribution: Contribution; nature: unknown };
export type ContributionReview = { accept: boolean; note: string };

export type Credit = {
  contributor: string;
  verified: number;
  submitted: number;
  rejected: number;
  verified_formalizations: number;
  metered_tokens: number;
  reported_tokens: number;
};

export type GrantStatus = 'active' | 'paused' | 'revoked';
export type DonationGrant = {
  donor: string;
  monthly_cap: number;
  model: string;
  period: string;
  used: number;
  status: GrantStatus;
  created_at: string;
  updated_at: string;
  revision: number;
};
export type DonationPatch = { monthly_cap?: number; status?: GrantStatus };
