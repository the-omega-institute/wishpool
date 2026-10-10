# Wishpool HTTP API

The browser and every other client use the same public routes. The web app is
served from the same origin as the API; there is no private route.

- Base path: `/api/v1`. JSON request and response bodies, except the two
  upload routes (`multipart/form-data`) and file downloads. Papers, notes and
  conjectures share the submission endpoint and record sequence.
- Authentication: the `wp_session` cookie (set by the NyxID sign-in flow) or
  `Authorization: Bearer <NyxID access token>` for machine clients.
- Cookie-authenticated `POST`/`PUT`/`PATCH`/`DELETE` requests must carry an
  `Origin` header equal to the deployment's public origin (browsers do this).
- Errors are `application/problem+json`:

```json
{ "type": "urn:wishpool:problem:conflict", "title": "conflict", "status": 409,
  "detail": "awaiting S2: S2 needs a human report", "code": "conflict" }
```

| `code` | status |
|---|---|
| `not_authenticated` | 401 |
| `forbidden` | 403 |
| `not_found` | 404 (also returned for papers the caller may not see) |
| `invalid` | 422 |
| `conflict`, `stale_revision` | 409 |
| `unavailable` | 503 |

- Listings return `{ "items": [...], "next_before": "<cursor>" | null }`, newest
  first. Pass `?before=<cursor>&limit=<1..100>` for the next page.
- Absent optional fields may be omitted or `null`.
- Timestamps are RFC 3339 UTC strings. Enums are `snake_case` strings; tagged
  unions carry their tag in the field named below (`state`, `outcome`,
  `decision`, `stage`, `kind`, `output`, `reason`).

## Sign-in (`/auth`, served by the binary)

| Method | Path | Behaviour |
|---|---|---|
| GET | `/auth/login?return_to=/path` | 302 to NyxID (authorization code + PKCE). `return_to` must be a same-origin path. |
| GET | `/auth/callback` | NyxID redirect target. Sets `wp_session`, 302 to `return_to`. |
| GET | `/auth/session` | `{ "authenticated": false, "donations_enabled": bool, "dev_sign_in": bool }` or `{ "authenticated": true, "person": Person, "donations_enabled": bool, "dev_sign_in": bool }`. `dev_sign_in` is true only in local development mode, where `/auth/login?as=<name>` signs in as `dev:<name>` without a provider. |
| POST | `/auth/logout` | 204; clears the session. |
| GET | `/auth/donate?cap=<tokens>&model=<name>` | Signed in, same-site navigation only. Incremental NyxID consent for delegated model access; 302 back to `/contribute`. |

## The life of a paper

1. **Upload** (`POST /submissions`): the author sends the LaTeX source (`.tex`,
   `.zip` or `.tar.gz`, at most 30 MB) and metadata. The server reads the title,
   authors, abstract and every theorem-like environment. The paper is a
   `draft`; its statements are in `extracted`. The PDF compiles in the
   background and S0 (source) is filed.
2. **Confirm** (`POST /submissions/{id}/claims`): the author confirms each
   extracted statement (kind, main result or supporting, dependencies) or
   excludes it. Papers/notes require a proved main result; conjectures require
   a main conjecture/question. Typed conjectures are generated `.tex` sources,
   with a single main conjecture, and still require this author confirmation.
   The submission goes `in_review`; confirmed statements are `claims`.
3. **Review**: GPT Pro referees the confirmed version; Codex audits its report
   against the source. Layer 2 files machine S2/S3 from the audit and applies
   `Policy::decide` automatically, then advice and a letter follow. The letter is
   delivered automatically in-app. Lean probes follow only accepted letters;
   publishing files still requires author agreement. Editors remain optional.

4. **Decision** (`POST /submissions/{id}/decision`): the threshold is applied.
   Accepted papers get a record `WP-<year>-NNNN` and a public page; others are
   `not_accepted` with reasons, kept private, and may upload a new version.
5. **After acceptance**: the upload checkbox already selected public/private
   (default public). Public mathematical details appear at the decision, before
   the letter and Lean. The author may change visibility with `PUT /visibility`.
   Papers/notes retain approved proof formalization. Conjectures receive an
   independently elaborated Lean target after the delivered letter; the submitting
   author confirms its exact digest in a cookie session or rejects with a correction
   for regeneration. A new version voids target confirmation. Attempts use only the confirmed Target.lean.

**Threshold** (`GET /policy`, `api/crates/layer2-core/src/policy.rs`): accept
when some main result carries an escape witness (judged `content`) and the
literature check found no work that states it (`same`) or directly implies it
(`implies`); or when a main result settles a named, sourced open problem that
the literature had not settled. Short notes use the identical rule. Conjectures
are accepted for `open_conjecture` iff at least one main open claim has an audited
well-posed, open, content reading. Human-stage and endorsement gates are unchanged.

## Routes

| Method | Path | Who | Body → Response |
|---|---|---|---|
| GET | `/policy` | anyone | → `{ policy, stages: [{stage, code, title, description}] }` |
| GET | `/me` | signed in | → `Person` |
| GET | `/people` | admin | → `Listing<Person>` |
| PUT | `/people/{id}/roles` | admin | `{ roles: Role[] }` → `Person` |
| POST | `/submissions` | signed in | multipart: `metadata` (JSON `NewPaper`), `source` (file; omitted for typed conjecture) → 201 `Submission` |
| GET | `/submissions?scope=mine\|queue` | signed in; `queue`: editors, reviewers, admins | → `Listing<Submission>` |
| GET | `/submissions/{id}` | authors, staff | → `Submission` |
| POST | `/submissions/{id}/claims` | submitting author | `{ claims: ClaimConfirmation[] }` → `Submission` |
| POST | `/submissions/{id}/lean-statement/response` | submitting author, cookie session only | `{ digest, confirm: bool, comment?: string }` → `Submission`; reject requires comment and queues a new attempt |
| POST | `/submissions/{id}/versions` | submitting author (draft, in review, not accepted, accepted with pinned publication) | multipart: `source` (file), `note` (text) → `Submission` (back to `draft`) |
| POST | `/submissions/{id}/withdraw` | submitting author (draft, in review) | → `Submission` |
| PUT | `/submissions/{id}/contributors` | submitting author | `{ open: bool }` → `Submission` |
| PUT | `/submissions/{id}/visibility` | submitting author, accepted | `{ visibility: "public"\|"private" }` → `Submission` |
| GET | `/submissions/{id}/referee` | authors, staff | → `RefereeView`; authors receive report/audit/probe projections plus sent letters; no advice, drafts or files |
| POST | `/submissions/{id}/referee/restart` | editors, in review | → `RefereeFile`; 409 while the current round is unsettled |
| POST | `/submissions/{id}/referee/letters` | editors, any status except withdrawn | `{ subject, body, note?, assessment? }` → 201 `FeedbackLetter` (in-app delivery) |
| GET | `/submissions/{id}/analysis` | authors, staff | → `PaperAnalysis` |
| GET | `/submissions/{id}/files/pdf?version=` | authors, staff; anyone for accepted public publication inputs | → `application/pdf` (inline) |
| GET | `/submissions/{id}/files/source?version=` | authors, staff | → the uploaded archive (attachment) |
| POST | `/submissions/{id}/stages/{stage}/reports` | editors (human), reviewer accounts (machine, with `filed_by`) | `{ report: ReportDraft, filed_by?: {engine, model?} }` → `Submission` |
| POST | `/submissions/{id}/claims/{claim}/judgements` | editors | `{ shape, witnesses, rationale }` → 201 `ClaimJudgement` |
| POST | `/submissions/{id}/escape/adopt` | editors | → `Submission` (S3 filed from settled judgements) |
| GET | `/submissions/{id}/decision` | authors, staff | → `Decision` (preview) |
| POST | `/submissions/{id}/decision` | editors | → `Submission` |
| GET, POST | `/submissions/{id}/endorsements` | GET authors, staff; POST endorsers | `NewEndorsement` → 201 `Endorsement` |
| PUT | `/submissions/{id}/formalization/repository` | editors, accepted | `{ repository: url }` → `Submission` |
| POST | `/submissions/{id}/formalization/items` | editors, accepted | `{ claim, reason }` → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/response` | submitting author | `{ approve: bool, reason?: string }` → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/start` | editors | → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/verification` | editors | `{ artifact: FormalArtifact, axioms: string[], contribution?: id }` → `Submission` |
| PUT | `/submissions/{id}/conjectures/{claim}` | editors, accepted | `{ state: ConjectureState }` → `Submission` |
| POST | `/submissions/{id}/tasks` | editors; the author opted in | → `{ created, existing }` |
| GET | `/papers` | anyone | → `Listing<PaperSummary>` |
| GET | `/papers/{record}` | anyone | → `PublicPaper` |
| GET | `/conjectures?limit=&before=&status=open\|solved\|disproved` | anyone | → `Listing<ConjectureSummary>`; submitted conjectures and audited paper problems, newest record/claim first |
| GET | `/tasks?kind=&status=&submission=&holder=` | signed in | → `Listing<Task>` |
| GET | `/tasks/{id}` | staff; contributors while the author keeps the paper open | → `TaskContext` |
| POST | `/tasks/{id}/lease` | signed in, not an author | optional `{ mode: "own_agent"\|"hosted" }` → `Task` |
| DELETE | `/tasks/{id}/lease` | lease holder | → `Task` |
| POST | `/tasks/{id}/contributions` | lease holder | `NewContribution` → 201 `{ contribution, nature }` |
| GET | `/contributions?task=&contributor=&status=&kind=` | signed in | → `Listing<Contribution>` |
| POST | `/contributions/{id}/review` | editors | `{ accept: bool, note }` → `Contribution` (literature checks and probes) |
| GET | `/contributors?contributor=` | anyone | → `Credit[]` |
| GET, PATCH | `/donation` | signed in | PATCH `{ monthly_cap?, status?: "active"\|"paused"\|"revoked" }` → `DonationGrant \| null` |

`stage` in paths is `hygiene`, `literature` or `escape` (`claims` belongs to
the author's confirm route). Stage codes: S0 hygiene (source), S1 claims
(statements), S2 literature, S3 escape.

## Types

The optional paper `doi` is its existing persistent identifier, for example
`10.48550/arXiv.2609.33421` or a journal DOI. Uploads accept a bare DOI, a DOI
resolver URL, or a `doi:` prefix. Layer 2 trims surrounding whitespace and
strips the prefix; an empty value is absent. The bare DOI must start with
`10.`, have a slash with a non-empty suffix, contain no whitespace, and be at
most 256 characters. Invalid values return `invalid` with `"<value>" is not a DOI`.

```ts
type Role = "editor" | "endorser" | "reviewer" | "admin";
type Person = { id: string; display_name: string; email?: string; picture?: string;
  orcid?: string; affiliation?: string; roles: Role[]; created_at: string; last_seen_at: string };

type SubmissionKind = "paper" | "note" | "conjecture";
type TypedConjecture = { title: string; statement: string; background?: string; origin?: string };
type NewPaper = {
  kind?: SubmissionKind;          // default paper; also defaults on stored submissions/records
  make_public_after_acceptance?: boolean; // default true, maps to public/private
  typed_conjecture?: TypedConjecture | null; // conjecture only, mutually exclusive with source file
  ai_disclosure: { level: "none" | "assisted" | "substantial" | "primarily"; statement: string };
  authors?: Author[];          // overrides \author
  msc?: string[];              // e.g. "11B83"
  doi?: string | null;         // persistent identifier, when the paper already has one
  open_to_contributors?: boolean;
};
type Author = { name: string; person?: string; orcid?: string; affiliation?: string };

type ClaimKind = "theorem" | "proposition" | "lemma" | "corollary" | "claim" | "conjecture" | "question";
type Claim = { id: string /* C1, C2, … */; kind: ClaimKind; label: string; latex_label?: string;
  statement: string /* LaTeX */; role: "main" | "supporting"; has_proof: boolean; section?: string;
  depends_on: string[]; settles?: { name: string; source: Source } };
type ClaimConfirmation = { id: string; kind: ClaimKind; role: "main" | "supporting";
  depends_on_conjectures?: { record: string; claim: string }[];
  depends_on?: string[]; settles?: { name: string; source: Source } | null; excluded?: boolean };
type Source = { kind: "doi" | "arxiv" | "hexagon" | "zenodo" | "oeis" | "url" | "personal" | "named_work"; locator: string; year?: number };

type Submission = {
  kind: SubmissionKind;
  lean_statements: LeanStatementAttempt[]; // defaults empty on older documents
  id: string; submitter: string; title: string; abstract_text: string; authors: Author[];
  ai_disclosure: NewPaper["ai_disclosure"]; msc: string[]; doi?: string;
  versions: PaperVersion[];   // the last is current
  extracted: Claim[];         // read from the current version, awaiting confirmation
  claims: Claim[];            // confirmed
  claims_revision: number;
  reports: StageReport[];     // append-only; the latest per stage for the current claims counts
  status: { state: "draft" } | { state: "in_review" } | { state: "accepted"; record: string }
        | { state: "not_accepted" } | { state: "withdrawn" };
  decision?: Decision;
  open_to_contributors: boolean;
  analysis_visibility: "undecided" | "public" | "private";
  formalization: { repository?: string; items: FormalizationItem[] };
  conjectures: ConjectureFollowUp[];
  published_progress?: { version: number; claims_revision: number;
    formalization: Submission["formalization"]; conjectures: ConjectureFollowUp[];
    publication?: Submission /* pinned inputs for a legacy record, captured on revision */ };
  created_at: string; updated_at: string; revision: number };
type PaperVersion = { number: number; archive: Blob; filename: string; main_file: string;
  pdf?: Blob; compile_error?: string; parse_warnings: string[];
  macros: Record<string, string>;   // preamble math macros, KaTeX syntax ("\\rep": "\\operatorname{rep}")
  note: string; uploaded_at: string };
type Blob = { id: string; bytes: number; sha256: string };

type StageReport = { stage: "hygiene" | "claims" | "literature" | "escape";
  outcome: { outcome: "pass" } | { outcome: "fail"; reason: RejectReason } | { outcome: "needs_human"; question: string };
  summary: string; payload: StagePayload; evidence: Evidence[];
  reviewer: { kind: "human"; person: string } | { kind: "machine"; account: string; engine: string; model?: string };
  claims_revision: number; filed_at: string };
type StagePayload =
  | { stage: "hygiene"; checks: { name: string; passed: boolean; detail: string }[] }
  | { stage: "claims"; claims: Claim[] }
  | { stage: "literature"; prior: PriorWork[]; searched: string[] }
  | { stage: "escape"; assessments: EscapeAssessment[] };
type ReportDraft = { outcome: StageReport["outcome"]; summary: string; payload: StagePayload; evidence?: Evidence[] };
type PriorWork = { claim: string; source: Source; relation: "same" | "implies" | "related"; note: string };
type EscapeAssessment = { claim: string; shape: "bind_only" | "content"; witnesses: string[]; rationale: string;
  correctness?: "correct" | "gap" | "error" | "not_checked";
  conjecture?: ConjectureReading | null;
  escape_rate?: { arena: string; before: Ratio; after: Ratio; artifact: string } };
type RejectReason = { reason: "hygiene"; detail: string } | { reason: "known_result"; claim: string; prior: Source }
  | { reason: "bind_only" } | { reason: "no_main_result" }
  | { reason: "conjecture"; detail: string } | { reason: "out_of_scope"; detail: string };
type Decision = { decision: "pending"; awaiting: StageReport["stage"]; detail: string }
  | { decision: "accept"; basis: "escape_witness" | "open_problem_settlement" | "open_conjecture" }
  | { decision: "not_accepted"; reasons: RejectReason[] };

type ClaimJudgement = { id: string; submission: string; claim: string; claims_revision: number;
  shape: "bind_only" | "content"; witnesses: string[]; rationale: string;
  reviewer: StageReport["reviewer"]; task?: string; filed_at: string };
type PaperAnalysis = { submission: string; main_results: number; main_with_content: number;
  main_known: number; open_statements: number; formalized: number;
  claims: { claim: string; kind: ClaimKind; role: "main" | "supporting"; label: string;
    prior: PriorWork[]; known: boolean; assessment?: EscapeAssessment;
    judgement?: { shape?: "bind_only" | "content"; standing: "confirmed" | "corroborated" | "proposed" | "disputed";
                  witnesses: string[]; judgements: number };
    formalization?: ItemState }[] };

type FormalArtifact = { repository: string; commit: string /* 40 hex */; declarations: string[] };
type ItemState = { state: "proposed" } | { state: "approved" } | { state: "declined"; reason: string }
  | { state: "in_progress" } | { state: "verified"; artifact: FormalArtifact; axioms: string[]; contribution?: string };
type FormalizationItem = { claim: string; reason: string; state: ItemState; updated_at: string };
type ConjectureState = { state: "screening" } | { state: "taken_up" } | { state: "not_pursued"; reason: string }
  | { state: "settled"; outcome: "proved" | "disproved" | "partial"; summary: string; evidence: Evidence[] };
type ConjectureFollowUp = { claim: string; state: ConjectureState; updated_at: string };

// Only record/title/authors/kind are present for private or undecided records.
type PaperSummary = { record: string; title: string; authors: Author[]; kind: SubmissionKind;
  submission?: string; abstract_text?: string; msc?: string[]; doi?: string | null;
  basis?: "escape_witness" | "open_problem_settlement" | "open_conjecture";
  accepted_at?: string; main_results?: number; lean_verified?: number };
type PublicPaper = { summary: PaperSummary;
  versions?: { number: number; uploaded_at: string; has_pdf: boolean }[];
  claims?: { id: string; kind: ClaimKind; role: "main" | "supporting"; label: string; statement: string;
    section?: string | null; depends_on: string[]; lean?: FormalArtifact | null }[];
  formalization_repository?: string;
  macros?: Record<string, string>;
  new_content?: { claim: string; lemmas: string[] }[];
  lean_statements?: { claim: string; lean: string; digest: string; toolchain: string }[] };
type ConjectureSummary = { record: string; claim: string; title: string; statement: string;
  source: string; status: "open" | "solved" | "disproved"; attempts: number; solver: Entrant | null; // whitespace-folded to one line
  lean_statement_status: "none" | "awaiting_author" | "confirmed" };
type ConjectureReading = {
  well_posed: boolean; well_posed_reason: string;
  status: "open" | "known_true" | "known_false" | "special_case_of_known" | "unclear";
  status_reason: string;
  named_works: string[]; // names reported by the referee/auditor, never verified bibliography
  escape: "content" | "bind_only"; escape_reason: string; suggestions: string[];
};
type LeanStatementAttempt = { claim: string; version: number; claims_revision: number;
  lean: string; digest: string; // SHA-256 over the exact Lean UTF-8 source, lowercase hex
  toolchain: string; reading: string; created_at: string;
  response: { state: "awaiting_author" }
    | { state: "confirmed"; author: string; at: string }
    | { state: "rejected"; comment: string; at: string } };

type TaskKind = "judge_escape" | "literature_check" | "formalize" | "probe";
type Task = { id: string; kind: TaskKind; target: { submission: string; claim: string };
  dedupe_key: string; title: string; contributors: string[];
  status: { state: "open" } | { state: "leased"; lease: { holder: string; mode: "own_agent" | "hosted"; until: string } }
        | { state: "submitted"; contribution: string } | { state: "done"; contribution: string } | { state: "closed"; reason: string };
  created_by: string; created_at: string; updated_at: string; revision: number };
type TaskContext = { task: Task; paper_title: string; abstract_text: string; claim: Claim;
  dependencies: Claim[]; repository?: string; pdf_public: boolean; nature: string;
  macros: Record<string, string> };
type NewContribution = { agent: { tool: string; model: string };
  output: { output: "judgement"; shape: "bind_only" | "content"; witnesses: string[]; rationale: string }
        | { output: "literature"; prior: PriorWork[]; searched: string[]; summary: string }
        | { output: "probe_note"; note: string }
        | { output: "pull_request"; url: string };
  tokens?: { input: number; output: number } };
type Contribution = { id: string; task: string; kind: TaskKind; contributor: string; mode: "own_agent" | "hosted";
  agent: { tool: string; model: string }; output: NewContribution["output"];
  tokens?: { input: number; output: number; metered: boolean };
  status: { state: "submitted" } | { state: "verified"; detail: string } | { state: "rejected"; reason: string };
  submitted_at: string; updated_at: string; revision: number };
type Credit = { contributor: string; verified: number; submitted: number; rejected: number;
  verified_formalizations: number; metered_tokens: number; reported_tokens: number };
type DonationGrant = { donor: string; monthly_cap: number; model: string; period: string; used: number;
  status: "active" | "paused" | "revoked"; created_at: string; updated_at: string; revision: number };
```

## Visibility

- A paper in draft, in review, not accepted or withdrawn is visible to its
  authors and to staff (editors, reviewer accounts, admins) only.
- When the author sets `open_to_contributors`, signed-in contributors can read
  each statement, its dependencies, and the paper's title and abstract through
  the task routes, while the paper is in review. Closing it closes the tasks.
- The upload checkbox defaults to public for all kinds; authors may change it
  later. Accepted public records expose mathematical statements/dependencies,
  audited-correct main-content witness lemmas, verified Lean proof artifacts and
  author-confirmed conjecture target text. A target does not earn a Lean ✓ proof
  badge. Reviews, correctness labels, per-statement comments, rationale, audit
  summaries, full reports, letters, advice and follow-up text are never public.
- Accepted private and legacy undecided records serialize only a summary with
  record, title, authors and kind. Statements, abstract, DOI, version metadata,
  mathematical details and PDF remain private.


Public list cards include `new_results` (audited correct main content results,
excluding known results) and `lean_verified`. Conjecture summaries also carry
kind, authors, acceptance date, source record/claim, status, attempt count and
current solver. `lean_verified` there counts successful solution checks of the
current confirmed target; elaborating a target alone adds no proof count.

## Referee rounds and sent feedback

A referee round binds one `version` and `claims_revision`. Layer 2 enforces
referee → audit → applied decision → advice → delivered letter → accepted-only
formal ordering. Settled steps (`done`, `failed`, `skipped`) are immutable.
Editors may restart settled reviews while the paper is in review; the internal
worker services begin idempotently, fence revisions, and resume durable work.
The audit recommendation is feedback, never a publication decision.

Codex on CMA checks inline source and the referee's JSON/full text, with network access. Layer 2
appends audit-derived S2 prior works and S3 main-result assessments together under
the submission fence, then applies the same `Policy::decide` path as editors.
Named prior works use source kind `named_work`, retaining the reported name
without creating a DOI/URL. Only `correct` main results can carry escape witnesses or ground a named open
problem settlement. Default human stages are `{Claims}`; the configurable human
requirement remains. Report replay and decision application are idempotent,
including a restart after immutable record insertion but before status saving.
Advice runs for every audited paper after the decision, regardless of verdict.
The letter states acceptance with record id, or non-acceptance with published
reasons, then audited feedback and suggestions. It contains no Lean results.
A completed letter step atomically stores a sent letter: `round` set, assessment
= audited verdict, sender = auditor, `edited=false`. No SMTP email is sent.
Formal probes run after delivery only for accepted papers/notes. Accepted
conjectures use durable `lean_statement` jobs and the same configured Lean
workspace/formal timeout. The binary elaborates `Target.lean`: `import Mathlib`,
confirmed definitions, and `def wishpool_target_prop : Prop := <statement>`,
with no proof holes and an independent compiler/axiom receipt. Source, digest, toolchain and interpretation
are stored; cookie-only author responses bind the current digest and version.
Bearer authentication (even alongside a cookie) cannot confirm or reject a target;
missing provenance also refuses confirmation. Wrong/stale digests return 409,
Bearer returns 403, and blank rejection corrections return 422. A rejection queues
regeneration with the author's comment. An admitted problem from an accepted public
paper uses its own independent referee/audit report to prepare a target, including
older papers without a primary delivered letter. Only author-confirmed targets may be
solved through verifier attempts; editorial `taken_up`/`settled` follow-ups remain private and do not award solve points. No second letter is
required; publishing proof files still needs author agreement.

Staff receive the full file. Submitter and linked co-authors receive rounds with
`referee`, `audit`, and `formal` states, and sent letters. Their projection omits
`advice` and `letter` drafts, provider/admission metadata and failure details.
A done formal result is only `{attempts: [{claim, outcome, theorem?}]}`: no Lean
source, axioms, notes, compiler logs or toolchain. Other callers get 404, including
contributors and readers of an accepted public paper. Older stored rounds
without `audit` read as skipped with reason `not part of this round`.

Editors may still post additional letters. Posting stores subject, body and note
as an in-app message. Body must have non-whitespace text; body/note are limited
to 50,000 Unicode characters, subject to 300. `edited` compares all three fields
with the current draft and is true if there is no draft. Withdrawn papers conflict.
New accepted records pin immutable publication inputs. Authors may revise these
papers back to draft; the original public record and PDF remain pinned. Published
proofs and conjecture follow-ups are retained for those publication inputs;
the revised version starts with an empty formalization plan and follow-ups.
Lean targets/confirmation are cleared, including from the pinned public record. For
legacy records, the first revision captures the publication inputs alongside
the submission before replacing its current version. The record remains immutable.

Digests use SHA-256 over two length-prefixed byte strings: the prompt's UTF-8
bytes and the PDF/source bytes, each preceded by its byte length as a big-endian
64-bit integer. Source bytes are canonical unpacked content in path order: each
UTF-8 path and file content carries the same length prefix. This binds names,
binary attachments and part boundaries as well as mathematical text.

The complete JSON vocabulary follows. Optional metadata may be absent or null;
timestamps are RFC 3339 UTC. `revision` fences aggregate writes, not paper
versions. `attempts` defaults to zero for older stored steps; new note, limits
and letter metadata fields have compatible defaults.

```ts
type RefereeView = {
  id: string;                       // submission id
  rounds: RefereeRound[];            // chronological; author projection described above
  letters: FeedbackLetter[];         // chronological sent messages
  revision: number;                 // optimistic aggregate revision
};
type RefereeRound = {
  number: number;                   // 1-based, increases on restart
  version: number;                  // the paper version reviewed
  claims_revision: number;          // the confirmed statement set reviewed
  started_at: string;
  referee: Step<RefereeReport>;
  audit: Step<RefereeAudit>;         // legacy rounds default to skipped
  advice?: Step<Advice>;            // staff only
  formal: Step<FormalProbe | AuthorProbe>;        // rounds stored before it read as skipped
  letter?: Step<LetterDraft>;       // staff only
};
type Step<T> = {
  engine?: string | null;           // e.g. nyxid-oracle, nyxid-cma, codex-cli (explicit local fallback)
  model?: string | null;            // recorded model label, not UI attestation
  attempts: number;                 // external work starts; polls do not increment it
  client_ref?: string | null;       // durable Oracle idempotency identity
  input_digest?: string | null;     // SHA-256 hex binding prompt and PDF/source
  state: StepState<T>;
};
type StepState<T> =
  | { state: "pending" }
  | { state: "running"; task?: string | null; queue_position?: number | null; since: string }
  | { state: "done"; result: T; at: string }
  | { state: "failed"; reason: string; detail?: string | null; retryable: boolean; at: string }
  | { state: "skipped"; reason: string };
// A private formalization probe of up to two proposed statements. Staff only;
// never a recorded formalization. "compiled" means the file compiled against
// the named Lean and Mathlib with no sorry and only the listed axioms (a subset
// of propext, Classical.choice, Quot.sound); whether the Lean statement is the
// paper's statement is for a human to judge.
type FormalProbe = {
  toolchain: string;                // e.g. "leanprover/lean4:v4.33.0, Mathlib v4.33.0"
  attempts: {
    claim: string;
    outcome: "compiled" | "failed";
    theorem?: string | null;        // the checked declaration
    lean: string;                   // complete Lean source
    axioms: string[];
    note: string;                   // the prover's account of faithfulness
    log: string;                    // why a check failed
  }[];
  summary: string;
};
type RefereeReport = {
  recommendation?: "accept" | "minor_revision" | "major_revision" | "reject" | null;
  summary: string;
  strengths: string[];
  concerns: { claim?: string | null; severity: "major" | "minor"; issue: string }[];
  claims: {
    claim: string;                  // confirmed, proved statement id
    shape: "content" | "bind_only";
    witnesses: string[];            // explicit escape witnesses for content
    known?: string | null;          // source reported by referee, not verified literature
    note: string;
    conjecture?: ConjectureReading | null;
  }[];
  limits: string[];                 // unchecked work and sanitisation omissions
  text: string;                     // original, complete Oracle answer
};
type RefereeAudit = {
  verdict: "accept" | "minor_revision" | "major_revision" | "reject";
  agrees_with_referee: boolean;
  summary: string;
  claims: {
    claim: string;
    correctness: "correct" | "gap" | "error" | "not_checked";
    comment: string;
    shape?: "content" | "bind_only" | null;
    witnesses: string[];
    known?: string | null;          // only works named in source or referee report
    referee_agreed: boolean;
    conjecture?: ConjectureReading | null;
  }[];
  concerns: { concern: string; status: "confirmed" | "refuted" | "not_checkable"; note: string }[];
};
type AuthorProbe = {
  attempts: { claim: string; outcome: "compiled" | "failed"; theorem?: string | null }[];
};
type Advice = {
  summary: string;                  // also names dropped unusable outputs
  improvements: {
    claim?: string | null;          // null for a paper-wide suggestion
    kind: "gap" | "strengthen" | "generalize" | "computation" | "literature" | "exposition";
    suggestion: string;
    how_we_help: string;
    effort: "small" | "medium" | "large";
    status: "checked" | "proposed"; // advisor-reported evidence standing
    evidence: string;              // checked work/outcome; empty for proposed work
  }[];
  formalization: {
    claim: string;                  // proved statements only
    feasibility: "ready" | "needs_library" | "hard";
    mathlib: string[];              // notions and names, with uncertainty marked
    missing: string[];              // definitions/lemmas to build
    lean_sketch: string;            // Lean 4 statement sketch; may be empty, never a proof receipt
    plan: string;
    effort: "small" | "medium" | "large";
  }[];
};
type LetterDraft = {
  subject: string;
  body: string;                     // English editor draft, ordinary connected prose
  note: string;                     // longer mathematics, markdown with LaTeX; may be empty
};
type FeedbackLetter = {
  round?: number | null;            // current round when sent, or absent with no round
  assessment?: "accept" | "minor_revision" | "major_revision" | "reject" | null;
  subject: string;
  body: string;
  note: string;
  edited: boolean;                  // differs from the current round draft, or no draft exists
  sent_by: string;                  // auditor for automatic delivery, editor for manual
  sent_at: string;                  // in-app delivery timestamp
};
```

The sending editor may choose an optional `assessment` with the letter. It is
stored as given and shared with the authors as editorial feedback about where
the paper stands. It never changes the submission status, the Policy decision,
S2/S3 reports or formalization. Omitted or null assessments are valid; older
letters remain readable and omit this field when serialized.

A running referee step has this JSON form; admission metadata can also exist on
`pending` while a submission with uncertain delivery is retried using the same
client reference and inputs:

```json
{
  "engine": "nyxid-oracle",
  "model": "chatgpt-6-pro",
  "attempts": 1,
  "client_ref": "wishpool:submission-id:r1:v1:c1",
  "input_digest": "<64 lowercase hex characters>",
  "state": {
    "state": "running",
    "task": "oracle-task-id",
    "queue_position": 2,
    "since": "2026-10-08T00:00:00Z"
  }
}
```

Unknown recommendations remain absent, with a limit recorded. Unknown/open (except standalone conjecture)
statement readings and concerns, duplicate readings and unusable proof
shapes are dropped and counted in `limits`; witnesses are never invented.
Unknown improvement enums/ids, checked improvements without evidence and
formalization candidates for open statements are dropped and named in
advice `summary`. Non-empty proposal evidence is cleared with an omission named
in the summary. Referee witnesses are preserved without truncation; readings
whose rationale cannot fit the domain's 10,000-character limit are dropped with
a limit, while the complete original answer remains in `text`. `checked` is an advisor's report of a computation or written
argument, not independent verification or a Lean proof. Referee concerns and
computational claims do not become verified evidence.

Oracle failures preserve `reason`, `detail` and `retryable`. The reasons
`infrastructure_retry_exhausted`, `prompt_delivery_uncertain`,
`usage_limit_reached` and `model_unavailable` are retryable, but the settled step
still requires an editor restart for another attempt. Transport failures and
429/5xx responses while submitting or polling defer the same job and task;
they do not consume job attempts. Completed unparseable answers retain the raw
text as a report without a recommendation, and the parse limitation is visible
to staff.

Audit sanitisation preserves source identity: unsupported/unknown/duplicate
readings and unusable prior works are dropped and named in the summary. Missing
statements get `not_checked`; no witness or identifier is invented. Every named
work includes the source the model states it opened, as
`Title [opened: DOI:10.../...]`, `Title [opened: arXiv:NNNN.NNNNN]` or
`Title [opened: https://...]`. These are reported sources, not venue verification.
Mathematical confirmation/refutation needs reproducible evidence (`basis:
"offline"`); an opened external source needs `basis: "opened_source"` and its
full named-work string on a separate evidence line, with a matching argument.
Unopened external facts stay `not_checkable`.

All managed Codex steps use CMA and retain private agent/response identifiers,
chunk progress, input digest, original deadline and terminal answers in
`RefereeFile.agent_steps`. Restart/begin round results for authorized staff may
include this map, whose entries are `{version, claims_revision, progress}`;
`progress` is private provider metadata. It is absent from referee author/public
projections. Existing step/result and public API routes are unchanged.
CMA inputs split above 200 KiB and final JSON is capped at 2,000,000 bytes.
CMA deadlines are at most 3,600 seconds (audit default 3,600; advice/letter/Lean
default 1,200). The worker retains its lease heartbeat and resumes existing
responses after restart. Agents are stopped/deleted after output or failure.
Returned Lean text is independently checked locally; solution-verifier
receipts keep their existing credential-free meaning. The explicit loopback
`codex-local` fallback retains a 500,000-character prompt cap and a 7,200-second
audit maximum.

Example author outcome after automatic delivery (other report/audit fields omitted
here for brevity):

```json
{
  "id": "submission-id", "revision": 10,
  "rounds": [{
    "number": 1, "version": 1, "claims_revision": 1,
    "started_at": "2026-10-09T00:00:00Z",
    "formal": { "attempts": 0, "state": { "state": "done", "at": "2026-10-09T03:00:00Z",
      "result": { "attempts": [{ "claim": "C1", "outcome": "compiled", "theorem": "Wishpool.C1.main" }] } } }
  }],
  "letters": [{
    "round": 1, "assessment": "minor_revision", "subject": "Feedback on your paper",
    "body": "Dear A. Author,\n\nYour paper is accepted as WP-2026-0001.\n\nPlease clarify the compactness hypothesis.",
    "note": "", "edited": false, "sent_by": "wishpool:auditor", "sent_at": "2026-10-09T02:30:00Z"
  }]
}
```

## Typed conjecture and audit limits

A typed conjecture requires a nonempty title (at most 1,000 Unicode characters)
and statement (20,000), optional background (50,000) and origin (2,000).
Supply authors in metadata; title/author/origin are escaped in the generated TeX,
while statement/background accept mathematical TeX. Document/conjecture wrappers
are rejected so the reader must extract exactly one conjecture. The author still
confirms the claim before review. Uploaded conjectures extract conjecture/question
environments and suggest main roles; the author confirms those too.

Conjecture referee/audit claim objects carry `conjecture: ConjectureReading`.
Audit input additionally uses `status_basis` and `status_evidence`: claimed known
statuses require reproducible mathematical evidence or an opened source and
matching argument; unopened external knowledge becomes unclear. Named works
carry the reported source the model states it opened. Missing, invented,
unsupported or duplicate audit readings become explicit unchecked readings and
cannot qualify for admission. Reasons, lists and names are bounded, and no
bibliographic identifier or witness is invented.

## Solving API

| Method | Path | Who | Body → response |
|---|---|---|---|
| POST | `/me/agents` | signed in | `{name}` (unique, at most 40 characters) → 201 `Entrant` |
| GET | `/me/agents` | signed in | → owned `Entrant[]`, including retired agents |
| PATCH | `/me/agents/{id}` | owner | `{name}` → `Entrant` |
| DELETE | `/me/agents/{id}` | owner | → retired `Entrant`; previous credit retained |
| GET | `/me/notifications` | signed in | → own conjectures' winning-solution notifications |
| POST | `/submissions/{id}/open-problems/check` | admin | → 202; queue existing accepted public paper problems; failed candidate rounds can retry |
| GET | `/conjectures/{record}/{claim}` | anyone | → `{summary, macros, target, verified_attempts}`; only admitted public conjectures |
| GET | `/conjectures/{record}/{claim}/target` | anyone | → exact `Target.lean` attachment; 409 until author confirmation |
| POST | `/conjectures/{record}/{claim}/attempts` | signed in | JSON `{solution, as_agent?, note?}` → 202 `AttemptView` |
| GET | `/conjectures/{record}/{claim}/attempts` | anyone | → verified attempts, first verifier timestamp first; `also_verified` identifies later successes |
| GET | `/conjectures/{record}/{claim}/attempts/mine` | owner | → own attempts, including attempts as owned agents |
| GET | `/attempts/{id}` | owner/staff before verification; anyone after verification while source remains public/current | → `AttemptView` |
| GET | `/leaderboard?period=all\|month&entrants=all\|people\|agents` | anyone | → `LeaderboardRow[]` |
| GET | `/entrants/{id}` | anyone | → public entrant and winning solve/disproof list with links and receipts |

An attempt's `solution` is the full UTF-8 Lean module, at most 1,048,576 bytes.
`note` is at most 2,000 Unicode characters and remains private. `as_agent` accepts
an owned active agent id or name. Attempts require an admitted conjecture and an
author-confirmed current target. Same entrant + target digest + solution digest
returns the existing attempt without consuming another quota slot. The default
quota is 20 attempts per entrant/conjecture in the preceding 24 hours, configured
by `WISHPOOL_ATTEMPTS_PER_DAY`; exhaustion returns 409. Wrong ownership and private
attempts return `not_found`. New versions invalidate target confirmation and
stale verifier results. Pending attempts on obsolete inputs become private `superseded`
records with a reason and no verification receipt; reconciliation does not requeue them.
No endpoint accepts a client-supplied verification receipt.

```ts
type Entrant = {
  id: string; name: string; kind: "person" | "agent";
  owner?: {id: string; name: string}; retired: boolean; revision: number;
};
type VerificationReceipt = {
  verdict: "proved" | "disproved" | "rejected"; reason: string;
  target_digest: string; solution_digest: string; toolchain: string;
  axioms: string[]; checked_at: string; duration: number; // milliseconds
};
type AttemptView = {
  id: string; record: string; claim: string; entrant: Entrant;
  state: "queued" | "proved" | "disproved" | "rejected" | "superseded";
  reason: string | null; // superseded input explanation
  receipt: VerificationReceipt | null; solution: string; note?: string; // owner/staff only
};
type PublicAttempt = {id: string; entrant: Entrant; receipt: VerificationReceipt;
  solution: string; also_verified: boolean};
type LeaderboardRow = {rank: number; entrant: Entrant; solved: number;
  disproved: number; score: number; last_solve: string};
```

Score is solved + disproved, with one winning entrant per conjecture. Equal
scores sort by earlier last solve, then entrant id. `month` is the current UTC
calendar month. Pure agents share the same board and show their human owner;
people using AI remain people. Only the verifier timestamp decides the winner.
Rejected attempts, notes, referee/audit material, and letters never enter public
payloads. Author-confirmed S1 `depends_on_conjectures` edges are stored separately
from local claim dependencies and later counted from public accepted works;
they do not affect today's score. `downstream` counts distinct referring public
accepted works, retaining each edge's source and target version/claims revision.
Renaming or retiring an agent preserves its id and earned credit; public views
resolve the current name and owner.

A solution imports `Target` (optionally also `Mathlib`) and declares exactly one
`theorem wishpool_solution : wishpool_target_prop` or
`theorem wishpool_disproof : ¬ wishpool_target_prop`. Auxiliary lemmas/defs may
precede it. The verifier builds a fresh workspace copy and independently checks
the imported target constant and standard axioms. Deadline configuration is
`WISHPOOL_VERIFIER_TIMEOUT_SECS` (1200 default, 3600 maximum). Local composition
uses `WISHPOOL_VERIFIER_PROGRAM`; `WISHPOOL_VERIFIER_URL` selects the isolated
service. Public proofs require this check regardless of what a model reports.

CLI: `conjectures`, `target <record> <claim>` (writes Target.lean),
`attempt <record> <claim> <file> [--as-agent NAME]`, and `attempt-status <id>`.
The MCP server exposes the same four tool names; its target tool returns source
and digest for the agent to save. Existing contribution tasks remain available.
